use std::ops::RangeInclusive;

pub fn is_ideograph(c: char) -> bool {
	use std::ops::RangeInclusive;
	const UNIFIED: RangeInclusive<char> = '\u{4E00}'..='\u{9FFF}';
	const UNIFIED_A: RangeInclusive<char> = '\u{3400}'..='\u{4DBF}';
	const UNIFIED_B: RangeInclusive<char> = '\u{20000}'..='\u{2A6DF}';
	const UNIFIED_CDEF: RangeInclusive<char> = '\u{2A700}'..='\u{2EBEF}';
	const UNIFIED_G: RangeInclusive<char> = '\u{30000}'..='\u{3134F}';
	const COMPATIBILITY: RangeInclusive<char> = '\u{F900}'..='\u{FAFF}';
	UNIFIED.contains(&c)
		|| UNIFIED_A.contains(&c)
		|| UNIFIED_B.contains(&c)
		|| UNIFIED_CDEF.contains(&c)
		|| UNIFIED_G.contains(&c)
		|| COMPATIBILITY.contains(&c)
		|| c == '\u{3005}'
}

const HIRA_0: RangeInclusive<char> = '\u{3041}'..='\u{3096}';
const HIRA_1: RangeInclusive<char> = '\u{309D}'..='\u{309F}';
const KATA_0: RangeInclusive<char> = '\u{30A1}'..='\u{30FA}';
const KATA_1: RangeInclusive<char> = '\u{30FC}'..='\u{30FF}';
const SMALL_HIRA_KO: char = '\u{1B132}';
const SMALL_KATA_KO: char = '\u{1B155}';
const SMALL_HIRA_WIEO: RangeInclusive<char> = '\u{1B150}'..='\u{1B152}';
const SMALL_KATA_WIEO: RangeInclusive<char> = '\u{1B164}'..='\u{1B167}';
const HIRA_YE: char = '\u{1B001}';
const KATA_YE: char = '\u{1B121}';
const COMBINING_SOUND_MARK: RangeInclusive<char> = '\u{3099}'..='\u{309A}';

pub fn try_consume_kana(c: char, chars: &mut std::str::Chars) -> Option<String> {
	if HIRA_0.contains(&c)
		|| HIRA_1.contains(&c)
		|| KATA_0.contains(&c)
		|| KATA_1.contains(&c)
		|| SMALL_HIRA_KO == c
		|| SMALL_KATA_KO == c
		|| SMALL_HIRA_WIEO.contains(&c)
		|| SMALL_KATA_WIEO.contains(&c)
		|| HIRA_YE == c
		|| KATA_YE == c
	{
		let mut buffer = c.to_string();
		buffer.extend(chars.clone().take_while(|x| COMBINING_SOUND_MARK.contains(x)).inspect(|_| {
			chars.next();
		}));
		Some(buffer)
	} else {
		None
	}
}

// Attempt to normalize a reading to katakana or ….
pub fn try_katakanify(
	reading: &str, should_ignore: impl Fn(char) -> bool, should_keep: impl Fn(char) -> bool,
) -> Option<String> {
	let mut normalized = String::with_capacity(reading.len());
	for c in reading.chars() {
		let c = match c {
			_ if should_ignore(c) => continue,
			_ if should_keep(c) => c,
			// NOTE: 江 has reading containing 𛀁.
			// NOTE: れ゚ has a combining diacritic.
			'\u{30A1}'..='\u{30FA}' | '\u{309A}'..='\u{309C}' | '\u{30FC}' | '\u{1B164}'..='\u{1B167}' => c,
			'\u{1B121}' | '\u{1B001}' => '\u{1B121}',
			'\u{1B132}' | '\u{1B155}' => '\u{1B155}',
			hira @ '\u{3041}'..='\u{3096}' => unsafe {
				char::from_u32_unchecked((hira as u32).unchecked_add(0x60))
			},
			ext @ '\u{1B150}'..='\u{1B152}' => unsafe {
				char::from_u32_unchecked((ext as u32).unchecked_add(0x14))
			},
			// '\u{309D}' | '\u{30FD}' | '\u{309E}' | '\u{30FE}' => panic!("{reading}: {c}"),
			'\u{309D}' | '\u{30FD}' => '\u{30FD}',
			'\u{309E}' | '\u{30FE}' => '\u{30FE}',
			_ => return None,
		};
		normalized.push(c);
	}
	Some(normalized)
}

pub fn expand_katakana(reading: &str) -> Option<String> {
	let mut chars = reading.chars();
	let mut kata_buffer = Vec::new();
	let mut buffer = String::new();
	while let Some(c) = chars.next() {
		if let Some(kana) = try_consume_kana(c, &mut chars) {
			match kana.as_str() {
				"\u{30FD}" | "\u{30FE}" => {
					let extra = reiterate(&kana, &kata_buffer, &mut chars)?;
					for kata in &kata_buffer {
						buffer.push_str(kata);
					}
					for kata in extra {
						buffer.push(kata);
					}
					kata_buffer.truncate(0);
				},
				_ => kata_buffer.push(kana),
			}
		} else {
			for kata in &kata_buffer {
				buffer.push_str(kata);
			}
			kata_buffer.truncate(0);
		}
	}
	for kata in &kata_buffer {
		buffer.push_str(kata);
	}
	Some(buffer)
}

// Parse a presumed nonempty sequence of kana iteration marks and attempt to normalize them into a buffer.
fn reiterate(first: &str, source: &[String], chars: &mut std::str::Chars) -> Option<Vec<char>> {
	let mut should_dakuten_by_mark = vec![matches!(first, "\u{30FE}")];
	let mut peek = chars.clone();
	while let Some(x) = peek.next() {
		if let Some(x) = try_consume_kana(x, &mut peek) {
			match x.as_str() {
				"\u{30FD}" => should_dakuten_by_mark.push(false),
				"\u{30FE}" => should_dakuten_by_mark.push(true),
				_ => break,
			}
		} else {
			break;
		}
	}
	let iteration_count = should_dakuten_by_mark.len();
	if iteration_count > source.len() {
		return None;
	}
	let source = &source[source.len() - iteration_count..];
	let mut target = Vec::with_capacity(iteration_count);
	for (original, should_dakuten) in source.iter().zip(should_dakuten_by_mark) {
		if should_dakuten {
			target.push(reiterate_dakuon(original)?);
		} else {
			target.push(reiterate_seion(original));
		}
	}
	for _ in 1..iteration_count {
		chars.next();
	}
	Some(target)
}

// Reproduce the given presumed katakana, removing its dakuten if present.
fn reiterate_seion(last: &str) -> char {
	let last = last.chars().next().unwrap();
	match last {
		'\u{30A1}'..='\u{30AA}'
		| '\u{30C3}'
		| '\u{30CA}'..='\u{30CE}'
		| '\u{30DE}'..='\u{30F3}'
		| '\u{30F5}'
		| '\u{30F6}' => last,
		'\u{30AB}'..='\u{30C2}' => {
			if !(last as u32).is_multiple_of(2) {
				last
			} else {
				unsafe { char::from_u32_unchecked((last as u32).unchecked_sub(1)) }
			}
		},
		'\u{30CF}'..='\u{30DD}' => unsafe {
			char::from_u32_unchecked((last as u32).unchecked_sub((last as u32 - 0x30CF) % 3))
		},
		'\u{30C4}'..='\u{30C9}' => {
			if (last as u32).is_multiple_of(2) {
				last
			} else {
				unsafe { char::from_u32_unchecked((last as u32).unchecked_sub(1)) }
			}
		},
		'\u{30F4}' => '\u{30A6}',
		'\u{30F7}'..='\u{30FA}' => unsafe { char::from_u32_unchecked((last as u32).unchecked_sub(8)) },
		..='\u{30A0}' | '\u{30FB}'.. => unimplemented!(),
	}
}

// Reproduce the given presumed katakana, attempting to attach a dakuten.
fn reiterate_dakuon(last: &str) -> Option<char> {
	let last = last.chars().next().unwrap();
	Some(match last {
		'\u{30A1}'..='\u{30A5}'
		| '\u{30A7}'..='\u{30AA}'
		| '\u{30C3}'
		| '\u{30CA}'..='\u{30CE}'
		| '\u{30DE}'..='\u{30EE}'
		| '\u{30F3}'
		| '\u{30F5}'
		| '\u{30F6}' => return None,
		'\u{30A6}' | '\u{30F4}' => '\u{30F4}',
		'\u{30AB}'..='\u{30C2}' => {
			if (last as u32).is_multiple_of(2) {
				last
			} else {
				unsafe { char::from_u32_unchecked((last as u32).unchecked_add(1)) }
			}
		},
		'\u{30CF}'..='\u{30DD}' => unsafe {
			char::from_u32_unchecked((last as u32).unchecked_add(2 - ((last as u32 - 0x30CF) % 3)))
		},
		'\u{30C4}'..='\u{30C9}' => {
			if !(last as u32).is_multiple_of(2) {
				last
			} else {
				unsafe { char::from_u32_unchecked((last as u32).unchecked_add(1)) }
			}
		},
		'\u{30EF}'..='\u{30F2}' => unsafe { char::from_u32_unchecked((last as u32).unchecked_add(8)) },
		'\u{30F7}'..='\u{30FA}' => last,
		..='\u{30A0}' | '\u{30FB}'.. => unimplemented!(),
	})
}

// Compute the length, in moras, of a string of presumed katakana.
pub fn compute_duration(kata_string: &str) -> usize {
	let mut duration = 0;
	for kata in kata_string.chars() {
		match kata {
			'\u{30A1}'
			| '\u{30A3}'
			| '\u{30A5}'
			| '\u{30A7}'
			| '\u{30A9}'
			| '\u{30E3}'
			| '\u{30E5}'
			| '\u{30E7}'
			| '\u{30EE}'
			| '\u{1B164}'..='\u{1B166}' => continue,
			_ => duration += 1,
		}
	}
	duration
}
