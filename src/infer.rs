use std::collections::HashSet;

use crate::{
	ja::{compute_duration, expand_katakana, is_ideograph, try_consume_kana, try_katakanify},
	parse::{JaKanjitab, JaPos, JaPron, JaPronAccent},
	wikitext::remove_links,
};

#[derive(Debug)]
pub struct DecompositionInfo {
	pub atoms: Vec<Atom>,
}

impl DecompositionInfo {
	pub fn reading(&self) -> String {
		self.atoms
			.iter()
			.map(|x| match x {
				Atom::Ruby { reading, .. } => reading.to_owned(),
				Atom::Unknown(c) => c.to_string(),
				Atom::Kana(kana) => kana.to_owned(),
			})
			.collect()
	}
}

#[derive(Debug)]
pub enum DecompositionError {
	Incomplete,   // The kanjitab is incomplete. This is likely an error in the source article.
	Empty,        // The kanjitab has no readings.
	Mismatch,     // The kanjitab does not match a reading. This is likely an error in the source article.
	Unconsidered, // Due to complications, this decomposition is ignored.
}

// A segment of a reading, consisting of a string of katakana and the number of characters it represents.
#[derive(Debug)]
pub enum Atom {
	Ruby { character_count: u8, reading: String },
	Unknown(char),
	Kana(String),
}

pub fn infer_decompositions(
	title: &str, ja_kanjitab: JaKanjitab, readings: &HashSet<String>,
) -> Result<DecompositionInfo, DecompositionError> {
	if ja_kanjitab.readings.is_empty() {
		if !(ja_kanjitab.alterations.is_empty() && ja_kanjitab.omissions.is_empty()) {
			println!("bad: {title}")
		}
		return Err(DecompositionError::Empty);
	}

	assert!(ja_kanjitab.readings.len() >= ja_kanjitab.alterations.len());
	assert!(ja_kanjitab.readings.len() >= ja_kanjitab.omissions.len());

	let mut atoms = Vec::new();
	let mut kanji_cursor = 0;

	let Some(kata_title) = try_katakanify(title, |c| matches!(c, '-' | '\u{3001}'), is_ideograph) else {
		return Err(DecompositionError::Unconsidered);
	};

	let mut chars = kata_title.chars();
	while let Some(c) = chars.next() {
		if is_ideograph(c) {
			let Some((reading, character_count)) = ja_kanjitab.readings.get(kanji_cursor) else {
				return Err(DecompositionError::Incomplete);
			};
			let reading =
				ja_kanjitab.alterations.get(kanji_cursor).and_then(Option::as_ref).unwrap_or(reading);
			let reading = if reading == "ー" || reading == "-" {
				// NOTE: "大元帥" and "鸕鷀草葺不合尊".
				"".to_owned()
			} else {
				try_katakanify(reading, |c| c.is_whitespace(), |_| false)
					.ok_or(DecompositionError::Unconsidered)?
			};
			atoms.push(Atom::Ruby { character_count: *character_count, reading: reading.clone() });
			if let Some(Some(omission)) = ja_kanjitab.omissions.get(kanji_cursor) {
				atoms.push(Atom::Ruby {
					character_count: 0,
					reading: try_katakanify(omission, |_| false, |_| false).unwrap(),
				});
			}
			for _ in 1..*character_count {
				let _kanji = chars.next().unwrap();
				assert!(is_ideograph(_kanji));
			}
			kanji_cursor += 1;
		} else if c == 'ヶ' {
			atoms.push(Atom::Unknown(c));
		} else if let Some(kana) = try_consume_kana(c, &mut chars) {
			atoms.push(Atom::Kana(kana));
		} else {
			unreachable!();
		}
	}

	assert!(
		atoms
			.iter()
			.map(|x| match x {
				Atom::Ruby { character_count, .. } => *character_count as u64,
				Atom::Unknown(_) => 0,
				Atom::Kana(_) => 0,
			})
			.sum::<u64>()
			== title.chars().map(is_ideograph).map(|x| x as u64).sum()
	);

	// NOTE: The presence of unused empty readings may indicate a non-fatal source error.
	assert!(ja_kanjitab.readings[kanji_cursor..].iter().all(|x| x.0.is_empty()));

	let Some(replacements) = align(&atoms, readings) else {
		return Err(DecompositionError::Mismatch);
	};

	for (i, reading) in replacements {
		atoms[i] = Atom::Ruby { character_count: 1, reading }
	}

	Ok(DecompositionInfo { atoms })
}

fn align(candidate: &[Atom], readings: &HashSet<String>) -> Option<Vec<(usize, String)>> {
	'reading: for reading in readings {
		let mut remaining = reading.as_str();
		let mut replacements = Vec::new();
		for (i, atom) in candidate.iter().enumerate() {
			match atom {
				Atom::Ruby { reading, .. } => {
					if let Some(then) = remaining.strip_prefix(reading.as_str()) {
						remaining = then
					} else {
						continue 'reading;
					}
				},
				Atom::Unknown(x) => match x {
					'ヶ' => {
						if let Some(then) = remaining.strip_prefix('カ') {
							remaining = then;
							replacements.push((i, "カ".to_owned()));
						} else if let Some(then) = remaining.strip_prefix('ガ') {
							remaining = then;
							replacements.push((i, "ガ".to_owned()));
						} else {
							continue 'reading;
						}
					},
					_ => continue 'reading,
				},
				Atom::Kana(kana) => {
					if let Some(then) = remaining.strip_prefix(kana.as_str()) {
						remaining = then;
					} else if kana == "ヅ"
						&& let Some(then) = remaining.strip_prefix("ズ")
					{
						remaining = then;
						replacements.push((i, "ズ".to_owned()));
					} else {
						continue 'reading;
					}
				},
			}
		}
		return Some(replacements);
	}
	None
}

pub fn pos_reading_ignore(c: char) -> bool {
	matches!(c, '.' | '%' | '-' | '\u{2010}' | '\u{30A0}' | '\u{30FB}' | '^' | '\'') || c.is_whitespace()
}

pub fn infer_pos_readings(ja_pos: JaPos) -> Vec<String> {
	let mut readings = Vec::new();
	for reading in ja_pos.readings {
		readings.extend(try_katakanify(&remove_links(&reading), pos_reading_ignore, |_| false));
	}
	readings
}

#[derive(Debug)]
pub struct AccentInfo {
	pub reading: String,
	pub accent: Option<u8>,
}

pub fn reading_ignore(c: char) -> bool {
	matches!(c, '.' | '%' | '-' | '\u{30A0}' | '\u{30FB}') || c.is_whitespace()
}

// Returns a list of kana readings (with duplicates) and an optional accent nucleus position for each.
pub fn infer_accent(title: &str, ja_pron: JaPron) -> Vec<AccentInfo> {
	enum Reading {
		Fallback,
		Error,
		Actual(String),
	}
	let mut readings = Vec::new();

	for reading in ja_pron.readings {
		if reading.is_empty() {
			readings.push(Reading::Fallback);
		} else {
			readings.push(
				try_katakanify(&reading, reading_ignore, |_| false)
					.and_then(|x| expand_katakana(&x))
					.map_or(Reading::Error, Reading::Actual),
			);
		}
	}

	let mut accents = ja_pron.accents;
	let max_len = readings.len().max(accents.len());
	readings.resize_with(max_len, || Reading::Fallback);
	accents.resize(max_len, JaPronAccent::None);

	// NOTE: Some such titles use iteration kana (いすゞ).
	let mut last_reading = try_katakanify(title, reading_ignore, |_| false).and_then(|x| expand_katakana(&x));

	let mut accent_infos = Vec::new();
	for (i, (reading, accent)) in readings.into_iter().zip(accents).enumerate() {
		let Some(reading) = (match reading {
			Reading::Error => {
				last_reading = None;
				continue;
			},
			Reading::Actual(x) => {
				last_reading = Some(x);
				&last_reading
			},
			Reading::Fallback => &last_reading,
		}) else {
			continue;
		};

		// Ignore non-Tokyo accents.
		if ja_pron.accent_locations.get(i).is_some_and(|x| *x) {
			continue;
		}

		let accent = match accent {
			JaPronAccent::Numeric(n) => Some(n),
			JaPronAccent::Odaka => Some(compute_duration(reading).try_into().unwrap()),
			JaPronAccent::None => None,
		};

		accent_infos.push(AccentInfo { reading: reading.clone(), accent })
	}

	assert!(accent_infos.iter().all(|i| i.accent.is_none_or(|a| a as usize <= compute_duration(&i.reading))));

	for a in &accent_infos {
		if a.reading.chars().any(|x| matches!(x, '\u{30FD}' | '\u{30FE}')) {
			println!("{title}, {}", a.reading);
		}
	}

	accent_infos
}
