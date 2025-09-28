mod infer;
mod ja;
mod parse;
mod wikitext;

use std::{
	collections::{HashMap, HashSet},
	fs::File,
	io::{BufReader, Read as _},
};

use crate::{
	infer::{Atom, infer_accent, infer_decompositions, infer_pos_readings},
	parse::{JaKanjitab, parse_ja_altread, parse_ja_kanjitab, parse_ja_pos, parse_ja_pron},
	wikitext::{FindTemplates, TemplateParameters},
};

fn main() {
	let input = File::open("scripts/enwiktionary-20250920/enwiktionary.bin").unwrap();
	let mut input = BufReader::new(input);
	let mut length_prefix = [0u8; 8];
	let mut info = HashMap::new();
	let mut redirects: Vec<Redirect> = Vec::new();
	loop {
		if input.read_exact(&mut length_prefix).is_err() {
			break;
		}
		let mut title = vec![0; u64::from_le_bytes(length_prefix) as _];
		input.read_exact(&mut title).unwrap();
		let title = String::from_utf8(title).unwrap();
		input.read_exact(&mut length_prefix).unwrap();
		let mut text = vec![0; u64::from_le_bytes(length_prefix) as _];
		input.read_exact(&mut text).unwrap();
		let text = String::from_utf8(text).unwrap();

		// Narrow text to Japanese section.
		// Assumes no fake new sections in a multi-line comment.
		const PREFIX: &str = "==Japanese==\n";
		let start = text.find(PREFIX).unwrap() + PREFIX.len();
		let mut text = &text[start..];
		for line in text.lines() {
			if line.len() > 2 && line.get(0..2) == Some("==") && line.as_bytes().get(2) != Some(&b'=') {
				text = &text[..unsafe { line.as_ptr().offset_from_unsigned(text.as_ptr()) }];
				break;
			}
		}

		// Split text by etymology if multiple such sections exist, then process each subtext.
		// NOTE: Sometimes, a text will have "Etymology 1" but only have one etymology. (e.g. 操)
		// NOTE: Sometimes, a text will have multiple "Etymology" sections. (e.g. 薄)
		// Assumes "===Etymology" does not appear in a comment somewhere.
		const ETYMOLOGY_PREFIX: &str = "===Etymology";
		if text.match_indices(ETYMOLOGY_PREFIX).map(|_| 1).sum::<u32>() > 1 {
			while let Some(i) = text.find(ETYMOLOGY_PREFIX) {
				text = &text[i + ETYMOLOGY_PREFIX.len()..];
				let i = text.find("===\n").unwrap() + "===\n".len();
				text = &text[i..];
				let mut current_text = text;
				for line in text.lines() {
					if line.len() > 3
						&& line.get(0..3) == Some("===")
						&& line.as_bytes().get(3) != Some(&b'=')
					{
						current_text = &current_text
							[..unsafe { line.as_ptr().offset_from_unsigned(current_text.as_ptr()) }];
						break;
					}
				}
				process(&title, current_text, &mut redirects, &mut info);
				text = &text[current_text.len()..];
			}
		} else {
			process(&title, text, &mut redirects, &mut info);
		}
	}

	'redirects: for redirect in redirects {
		for see in redirect.sees {
			let readings = {
				let Some(word_info) = info.get(&see) else {
					continue;
				};
				word_info.readings.clone()
			};

			for ja_kanjitab in redirect.ja_kanjitabs {
				let Ok(decomposition) = infer_decompositions(&redirect.title, ja_kanjitab, &readings) else {
					continue;
				};
				let new_info = info
					.entry(redirect.title.clone())
					.or_insert_with(|| WordInfo { reading_infos: HashMap::new(), readings: HashSet::new() });

				let reading = decomposition.reading();
				let reading_info = new_info
					.reading_infos
					.entry(reading)
					.or_insert(ReadingInfo { accents: Vec::new(), decomposition: None });
				// NOTE: Assumes duplicate decompositions (e.g. 綽約) are identical.
				if reading_info.decomposition.is_none() {
					reading_info.decomposition = Some(decomposition.atoms);
				}
			}

			continue 'redirects;
		}
	}

	assert!(info.iter().all(|(_, info)| {
		info.reading_infos.iter().all(|(_, info)| !info.accents.is_empty() || info.decomposition.is_some())
	}));

	println!("{}", info.len());
	// for (title, info) in info {
	// 	for (reading, info) in info.reading_infos {
	// 		println!("{title}.{reading}: {:?} + {:?}", info.accents, info.decomposition);
	// 	}
	// }
}

struct Redirect {
	title: String,
	// NOTE: There may be multiple redirects and multiple kanji tables (see 米[メートル|メーター]).
	ja_kanjitabs: Vec<JaKanjitab>,
	sees: Vec<String>,
}

struct WordInfo {
	reading_infos: HashMap<String, ReadingInfo>,
	readings: HashSet<String>,
}

struct ReadingInfo {
	accents: Vec<u8>,
	decomposition: Option<Vec<Atom>>,
}

fn process(title: &str, text: &str, redirects: &mut Vec<Redirect>, info: &mut HashMap<String, WordInfo>) {
	let mut sees: Vec<String> = Vec::new();
	let mut ja_prons = Vec::new();
	let mut ja_kanjitabs = Vec::new();
	let mut ja_poss = Vec::new();

	for (name, arguments) in FindTemplates::new(text) {
		match name {
			"ja-romaji" | "ja-rom" => return,
			"ja-see" | "ja-see-kango" | "ja-gv" => {
				sees.extend(TemplateParameters::new(arguments).map(|x| x.to_string()).collect::<Vec<_>>())
			},
			"ja-pron" => ja_prons.push(parse_ja_pron(arguments).unwrap()),
			"ja-kanjitab" => ja_kanjitabs.extend(parse_ja_kanjitab(arguments)),
			"ja-pos" => ja_poss.push(parse_ja_pos(true, arguments)),
			"ja-noun" | "ja-verb" | "ja-verb form" | "ja-verb-suru" | "ja-adj" | "ja-phrase" => {
				ja_poss.push(parse_ja_pos(false, arguments))
			},
			"ja-altread" => ja_poss.push(parse_ja_altread(arguments)),
			_ => continue,
		}
	}

	if !sees.is_empty() && (ja_poss.is_empty() && ja_prons.is_empty()) {
		if !ja_kanjitabs.is_empty() {
			redirects.push(Redirect { title: title.to_owned(), ja_kanjitabs, sees });
		}
		return;
	}

	let word_info = info
		.entry(title.to_owned())
		.or_insert(WordInfo { reading_infos: HashMap::new(), readings: HashSet::new() });

	let mut readings = HashSet::new();
	for ja_pron in ja_prons {
		for info in infer_accent(title, ja_pron) {
			readings.insert(info.reading.clone());
			if let Some(accent) = info.accent {
				word_info
					.reading_infos
					.entry(info.reading)
					.or_insert(ReadingInfo { accents: Vec::new(), decomposition: None })
					.accents
					.push(accent);
			}
		}
	}

	for ja_pos in ja_poss {
		readings.extend(infer_pos_readings(ja_pos));
	}

	word_info.readings.extend(readings);

	for ja_kanjitab in ja_kanjitabs {
		if let Ok(decomposition) = infer_decompositions(title, ja_kanjitab, &word_info.readings) {
			let reading = decomposition.reading();
			let reading_info = word_info
				.reading_infos
				.entry(reading)
				.or_insert(ReadingInfo { accents: Vec::new(), decomposition: None });
			// NOTE: Assumes duplicate decompositions are identical.
			if reading_info.decomposition.is_none() {
				reading_info.decomposition = Some(decomposition.atoms);
			}
		}
	}
}
