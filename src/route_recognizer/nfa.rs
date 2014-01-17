extern mod extra;
use std::hashmap::HashSet;
use extra::treemap::TreeSet;
use std::u64;

#[deriving(Eq, Clone)]
struct CharSet {
  low_mask: u64,
  high_mask: u64,
  non_ascii: HashSet<char>
}

impl CharSet {
  pub fn new() -> CharSet {
    CharSet{ low_mask: 0, high_mask: 0, non_ascii: HashSet::new() }
  }

  pub fn insert(&mut self, char: char) {
    let val = char as u64 - 1;

    if val > 127 {
      self.non_ascii.insert(char);
    } else if val > 63 {
      let bit = 1 << val - 64;
      self.high_mask = self.high_mask | bit;
    } else {
      let bit = 1 << val;
      self.low_mask = self.low_mask | bit;
    }
  }

  pub fn contains(&self, char: char) -> bool {
    let val = char as u64 - 1;

    if val > 127 {
      self.non_ascii.contains(&char)
    } else if val > 63 {
      let bit = 1 << val - 64;
      self.high_mask & bit != 0
    } else {
      let bit = 1 << val;
      self.low_mask & bit != 0
    }
  }
}

#[deriving(Eq, Clone)]
pub enum CharacterClass {
  Ascii(u64, u64, bool),
  ValidChars(CharSet),
  InvalidChars(CharSet)
}

impl CharacterClass {
  pub fn any() -> CharacterClass {
    Ascii(u64::max_value, u64::max_value, true)
  }

  pub fn valid(string: &str) -> CharacterClass {
    ValidChars(CharacterClass::str_to_set(string))
  }

  pub fn invalid(string: &str) -> CharacterClass {
    InvalidChars(CharacterClass::str_to_set(string))
  }

  pub fn valid_char(char: char) -> CharacterClass {
    let val = char as u64 - 1;

    if val > 127 {
      ValidChars(CharacterClass::char_to_set(char))
    } else if val > 63 {
      Ascii(1 << val - 64, 0, false)
    } else {
      Ascii(0, 1 << val, false)
    }
  }

  pub fn invalid_char(char: char) -> CharacterClass {
    let val = char as u64 - 1;

    if val > 127 {
      InvalidChars(CharacterClass::char_to_set(char))
    } else if val > 63 {
      Ascii(u64::max_value ^ (1 << val - 64), u64::max_value, true)
    } else {
      Ascii(u64::max_value, u64::max_value ^ (1 << val), true)
    }
  }


  pub fn matches(&self, char: char) -> bool {
    match *self {
      ValidChars(ref valid) => valid.contains(char),
      InvalidChars(ref invalid) => !invalid.contains(char),
      Ascii(high, low, unicode) => {
        let val = char as u64 - 1;
        if val > 127 {
          unicode
        } else if val > 63 {
          high & (1 << (val - 64)) != 0
        } else {
          low & (1 << val) != 0
        }
      }
    }
  }

  fn char_to_set(char: char) -> CharSet {
    let mut set = CharSet::new();
    set.insert(char);
    set
  }

  fn str_to_set(string: &str) -> CharSet {
    let mut set = CharSet::new();
    for char in string.chars() {
      set.insert(char);
    }
    set
  }
}

#[deriving(Clone)]
struct Thread {
  state: uint,
  captures: ~[(uint, uint)],
  capture_begin: Option<uint>
}

impl Thread {
  pub fn new() -> Thread {
    Thread{ state: 0, captures: ~[], capture_begin: None }
  }

  #[inline]
  pub fn start_capture(&mut self, start: uint) {
    self.capture_begin = Some(start);
  }

  #[inline]
  pub fn end_capture(&mut self, end: uint) {
    self.captures.push((self.capture_begin.unwrap(), end));
    self.capture_begin = None;
  }

  pub fn extract<'a>(&self, source: &'a str) -> ~[&'a str] {
    self.captures.iter().map(|&(begin, end)| source.slice(begin, end)).collect()
  }
}

#[deriving(Clone)]
struct State<T> {
  index: uint,
  chars: CharacterClass,
  next_states: ~[uint],
  acceptance: bool,
  start_capture: bool,
  end_capture: bool,
  metadata: Option<T>
}

impl<T> Eq for State<T> {
  fn eq(&self, other: &State<T>) -> bool {
    self.index == other.index
  }
}

impl<T> State<T> {
  pub fn new(index: uint, chars: CharacterClass) -> State<T> {
    State{ index: index, chars: chars, next_states: ~[], acceptance: false, start_capture: false, end_capture: false, metadata: None }
  }
}

pub struct Match<'a> {
  state: uint,
  captures: ~[&'a str]
}

impl<'a> Match<'a> {
  pub fn new<'a>(state: uint, captures: ~[&'a str]) -> Match<'a> {
    Match{ state: state, captures: captures }
  }
}

#[deriving(Clone)]
pub struct NFA<T> {
  states: ~[State<T>],
  start_capture: ~[bool],
  end_capture: ~[bool],
  acceptance: ~[bool]
}

impl<T> NFA<T> {
  pub fn new() -> NFA<T> {
    let root = State::new(0, CharacterClass::any());
    NFA{ states: ~[root], start_capture: ~[false], end_capture: ~[false], acceptance: ~[false] }
  }

  pub fn process<'a, I: Ord>(&self, string: &'a str, ord: |index: uint| -> I) -> Result<Match<'a>, ~str> {
    let mut threads = ~[Thread::new()];

    for (i, char) in string.chars().enumerate() {
      let next_threads = self.process_char(threads, char, i);

      if next_threads.is_empty() {
        return Err("Couldn't process " + string);
      }

      threads = next_threads;
    }

    let mut returned = threads.move_iter().filter(|thread| {
      self.get(thread.state).acceptance
    });

    let mut thread = returned.max_by(|thread| ord(thread.state));

    match thread {
      None => Err(~"The string was exhausted before reaching an acceptance state"),
      Some(mut thread) => {
        if thread.capture_begin.is_some() { thread.end_capture(string.len()); }

        let state = self.get(thread.state);
        Ok(Match::new(state.index, thread.extract(string)))
      }
    }
  }

  #[inline]
  fn process_char<'a>(&self, threads: ~[Thread], char: char, pos: uint) -> ~[Thread] {
    let mut returned = ::std::vec::with_capacity(threads.len());

    for mut thread in threads.move_iter() {
      let current_state = self.get(thread.state);

      let mut count = 0;
      let mut found_state = 0;

      for &index in current_state.next_states.iter() {
        let state = &self.states[index];

        if state.chars.matches(char) {
          count += 1;
          found_state = index;
        }
      }

      if count == 1 {
        thread.state = found_state;
        capture(self, &mut thread, current_state.index, found_state, pos);
        returned.push(thread);
        continue;
      }

      for &index in current_state.next_states.iter() {
        let state = &self.states[index];
        if state.chars.matches(char) {
          let mut thread = fork_thread(&thread, state);
          capture(self, &mut thread, current_state.index, index, pos);
          returned.push(thread);
        }
      }

    }

    returned
  }

  #[inline]
  pub fn get<'a>(&'a self, state: uint) -> &'a State<T> {
    &self.states[state]
  }

  pub fn get_mut<'a>(&'a mut self, state: uint) -> &'a mut State<T> {
    &mut self.states[state]
  }

  pub fn put(&mut self, index: uint, chars: CharacterClass) -> uint {
    {
      let state = self.get(index);

      for &index in state.next_states.iter() {
        let state = self.get(index);
        if state.chars == chars {
          return index;
        }
      }
    }

    let state = self.new_state(chars);
    self.get_mut(index).next_states.push(state);
    state
  }

  pub fn put_state(&mut self, index: uint, child: uint) {
    self.get_mut(index).next_states.push(child);
  }

  pub fn acceptance(&mut self, index: uint) {
    self.get_mut(index).acceptance = true;
    self.acceptance[index] = true;
  }

  pub fn start_capture(&mut self, index: uint) {
    self.get_mut(index).start_capture = true;
    self.start_capture[index] = true;
  }

  pub fn end_capture(&mut self, index: uint) {
    self.get_mut(index).end_capture = true;
    self.end_capture[index] = true;
  }

  pub fn metadata(&mut self, index: uint, metadata: T) {
    self.get_mut(index).metadata = Some(metadata);
  }

  fn new_state(&mut self, chars: CharacterClass) -> uint {
    let index = self.states.len();
    let state = State::new(index, chars);
    self.states.push(state);

    self.acceptance.push(false);
    self.start_capture.push(false);
    self.end_capture.push(false);

    index
  }
}

#[inline]
fn fork_thread<T>(thread: &Thread, state: &State<T>) -> Thread {
  let mut new_trace = thread.clone();
  new_trace.state = state.index;
  new_trace
}

#[inline]
fn capture<T>(nfa: &NFA<T>, thread: &mut Thread, current_state: uint, next_state: uint, pos: uint) {
  if thread.capture_begin == None && nfa.start_capture[next_state] {
    thread.start_capture(pos);
  }

  if thread.capture_begin != None && nfa.end_capture[current_state] && next_state > current_state {
    thread.end_capture(pos);
  }
}

#[test]
fn basic_test() {
  let mut nfa = NFA::<()>::new();
  let a = nfa.put(0, CharacterClass::valid("h"));
  let b = nfa.put(a, CharacterClass::valid("e"));
  let c = nfa.put(b, CharacterClass::valid("l"));
  let d = nfa.put(c, CharacterClass::valid("l"));
  let e = nfa.put(d, CharacterClass::valid("o"));
  nfa.acceptance(e);

  let m = nfa.process("hello", |a| a);

  assert!(m.unwrap().state == e, "You didn't get the right final state");
}

#[test]
fn multiple_solutions() {
  let mut nfa = NFA::<()>::new();
  let a1 = nfa.put(0, CharacterClass::valid("n"));
  let b1 = nfa.put(a1, CharacterClass::valid("e"));
  let c1 = nfa.put(b1, CharacterClass::valid("w"));
  nfa.acceptance(c1);

  let a2 = nfa.put(0, CharacterClass::invalid(""));
  let b2 = nfa.put(a2, CharacterClass::invalid(""));
  let c2 = nfa.put(b2, CharacterClass::invalid(""));
  nfa.acceptance(c2);

  let m = nfa.process("new", |a| a);

  assert!(m.unwrap().state == c2, "The two states were not found");
}

#[test]
fn multiple_paths() {
  let mut nfa = NFA::<()>::new();
  let a = nfa.put(0, CharacterClass::valid("t"));   // t
  let b1 = nfa.put(a, CharacterClass::valid("h"));  // th
  let c1 = nfa.put(b1, CharacterClass::valid("o")); // tho
  let d1 = nfa.put(c1, CharacterClass::valid("m")); // thom
  let e1 = nfa.put(d1, CharacterClass::valid("a")); // thoma
  let f1 = nfa.put(e1, CharacterClass::valid("s")); // thomas

  let b2 = nfa.put(a, CharacterClass::valid("o"));  // to
  let c2 = nfa.put(b2, CharacterClass::valid("m")); // tom

  nfa.acceptance(f1);
  nfa.acceptance(c2);

  let thomas = nfa.process("thomas", |a| a);
  let tom = nfa.process("tom", |a| a);
  let thom = nfa.process("thom", |a| a);
  let nope = nfa.process("nope", |a| a);

  assert!(thomas.unwrap().state == f1, "thomas was parsed correctly");
  assert!(tom.unwrap().state == c2, "tom was parsed correctly");
  assert!(thom.is_err(), "thom didn't reach an acceptance state");
  assert!(nope.is_err(), "nope wasn't parsed");
}

#[test]
fn repetitions() {
  let mut nfa = NFA::<()>::new();
  let a = nfa.put(0, CharacterClass::valid("p"));   // p
  let b = nfa.put(a, CharacterClass::valid("o"));   // po
  let c = nfa.put(b, CharacterClass::valid("s"));   // pos
  let d = nfa.put(c, CharacterClass::valid("t"));   // post
  let e = nfa.put(d, CharacterClass::valid("s"));   // posts
  let f = nfa.put(e, CharacterClass::valid("/"));   // posts/
  let g = nfa.put(f, CharacterClass::invalid("/")); // posts/[^/]
  nfa.put_state(g, g);

  nfa.acceptance(g);

  let post = nfa.process("posts/1", |a| a);
  let new_post = nfa.process("posts/new", |a| a);
  let invalid = nfa.process("posts/", |a| a);

  assert!(post.unwrap().state == g, "posts/1 was parsed");
  assert!(new_post.unwrap().state == g, "posts/new was parsed");
  assert!(invalid.is_err(), "posts/ was invalid");
}

#[test]
fn repetitions_with_ambiguous() {
  let mut nfa = NFA::<()>::new();
  let a  = nfa.put(0, CharacterClass::valid("p"));   // p
  let b  = nfa.put(a, CharacterClass::valid("o"));   // po
  let c  = nfa.put(b, CharacterClass::valid("s"));   // pos
  let d  = nfa.put(c, CharacterClass::valid("t"));   // post
  let e  = nfa.put(d, CharacterClass::valid("s"));   // posts
  let f  = nfa.put(e, CharacterClass::valid("/"));   // posts/
  let g1 = nfa.put(f, CharacterClass::invalid("/")); // posts/[^/]
  let g2 = nfa.put(f, CharacterClass::valid("n"));   // posts/n
  let h2 = nfa.put(g2, CharacterClass::valid("e"));  // posts/ne
  let i2 = nfa.put(h2, CharacterClass::valid("w"));  // posts/new

  nfa.put_state(g1, g1);

  nfa.acceptance(g1);
  nfa.acceptance(i2);

  let post = nfa.process("posts/1", |a| a);
  let ambiguous = nfa.process("posts/new", |a| a);
  let invalid = nfa.process("posts/", |a| a);

  assert!(post.unwrap().state == g1, "posts/1 was parsed");
  assert!(ambiguous.unwrap().state == i2, "posts/new was ambiguous");
  assert!(invalid.is_err(), "posts/ was invalid");
}

#[test]
fn captures() {
  let mut nfa = NFA::<()>::new();
  let a = nfa.put(0, CharacterClass::valid("n"));
  let b = nfa.put(a, CharacterClass::valid("e"));
  let c = nfa.put(b, CharacterClass::valid("w"));

  nfa.acceptance(c);
  nfa.start_capture(a);
  nfa.end_capture(c);

  let post = nfa.process("new", |a| a);

  assert_eq!(post.unwrap().captures, ~[&"new"]);
}

#[test]
fn capture_mid_match() {
  let mut nfa = NFA::<()>::new();
  let a = nfa.put(0, valid('p'));
  let b = nfa.put(a, valid('/'));
  let c = nfa.put(b, invalid('/'));
  let d = nfa.put(c, valid('/'));
  let e = nfa.put(d, valid('c'));

  nfa.put_state(c, c);
  nfa.acceptance(e);
  nfa.start_capture(c);
  nfa.end_capture(c);

  let post = nfa.process("p/123/c", |a| a);

  assert_eq!(post.unwrap().captures, ~[&"123"]);
}

#[test]
fn capture_multiple_captures() {
  let mut nfa = NFA::<()>::new();
  let a = nfa.put(0, valid('p'));
  let b = nfa.put(a, valid('/'));
  let c = nfa.put(b, invalid('/'));
  let d = nfa.put(c, valid('/'));
  let e = nfa.put(d, valid('c'));
  let f = nfa.put(e, valid('/'));
  let g = nfa.put(f, invalid('/'));

  nfa.put_state(c, c);
  nfa.put_state(g, g);
  nfa.acceptance(g);

  nfa.start_capture(c);
  nfa.end_capture(c);

  nfa.start_capture(g);
  nfa.end_capture(g);

  let post = nfa.process("p/123/c/456", |a| a);
  assert_eq!(post.unwrap().captures, ~[&"123", &"456"]);
}

#[test]
fn test_ascii_set() {
  let mut set = CharSet::new();
  set.insert('?');
  set.insert('a');
  set.insert('é');

  assert!(set.contains('?'), "The set contains char 63");
  assert!(set.contains('a'), "The set contains char 97");
  assert!(set.contains('é'), "The set contains char 233");
  assert!(!set.contains('q'), "The set does not contain q");
  assert!(!set.contains('ü'), "The set does not contain ü");
}

#[bench]
fn bench_char_set(b: &mut extra::test::BenchHarness) {
  let mut set = CharSet::new();
  set.insert('p');
  set.insert('n');
  set.insert('/');

  b.iter(|| {
    assert!(set.contains('p'))
    assert!(set.contains('/'));
    assert!(!set.contains('z'));
  });
}

#[bench]
fn bench_hash_set(b: &mut extra::test::BenchHarness) {
  let mut set = HashSet::new();
  set.insert('p');
  set.insert('n');
  set.insert('/');

  b.iter(|| {
    assert!(set.contains(&'p'));
    assert!(set.contains(&'/'));
    assert!(!set.contains(&'z'));
  });
}

#[bench]
fn bench_tree_set(b: &mut extra::test::BenchHarness) {
  let mut set = TreeSet::new();
  set.insert('p');
  set.insert('n');
  set.insert('/');

  b.iter(|| {
    assert!(set.contains(&'p'));
    assert!(set.contains(&'/'));
    assert!(!set.contains(&'z'));
  });
}

#[allow(dead_code)]
fn valid(char: char) -> CharacterClass {
  CharacterClass::valid_char(char)
}

#[allow(dead_code)]
fn invalid(char: char) -> CharacterClass {
  CharacterClass::invalid_char(char)
}

