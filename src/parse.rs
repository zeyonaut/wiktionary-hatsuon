use crate::wikitext::TemplateParameters;

pub struct JaKanjitab {
	pub readings: Vec<(String, u8)>,
	pub alterations: Vec<Option<String>>,
	pub omissions: Vec<Option<String>>,
}

pub fn parse_ja_kanjitab(arguments: &str) -> Option<JaKanjitab> {
	let mut readings = Vec::new();
	let mut alterations = Vec::new();
	let mut omissions = Vec::new();

	for argument in TemplateParameters::new(arguments) {
		if let Some((parameter, value)) = argument.split_once('=') {
			match parse_ja_kanjitab_parameter(parameter) {
				None => continue,
				Some(JaKanjitabParameter::Alter(n)) => {
					let n = n.strict_sub(1) as usize;
					if n >= alterations.len() {
						alterations.resize(n + 1, None);
					}
					assert!(alterations[n].is_none());
					alterations[n] = Some(value.to_owned());
				},
				Some(JaKanjitabParameter::Omit(n)) => {
					let n = n.strict_sub(1) as usize;
					if n >= omissions.len() {
						omissions.resize(n + 1, None);
					}
					assert!(omissions[n].is_none());
					omissions[n] = Some(value.to_owned());
				},
				Some(JaKanjitabParameter::Reading(n)) => {
					assert!(n as usize > readings.len());
					readings.resize(n.strict_sub(1) as usize, ("".to_owned(), 1));
					let (reading, count) = cut(value, |c: char| c.is_ascii_digit());
					let count = if count.is_empty() { 1 } else { count.parse().unwrap() };
					readings.push((reading.to_string(), count));
				},
			}
		} else {
			let (reading, count) = cut(&argument, |c: char| c.is_ascii_digit());
			let count = if count.is_empty() { 1 } else { count.parse().unwrap() };
			readings.push((reading.to_string(), count));
		}
	}

	// NOTE: かすか.
	if readings.is_empty() && alterations.is_empty() && omissions.is_empty() {
		return None;
	}

	Some(JaKanjitab { readings, alterations, omissions })
}

enum JaKanjitabParameter {
	Alter(u8),
	Omit(u8),
	Reading(u8),
}

fn parse_ja_kanjitab_parameter(parameter: &str) -> Option<JaKanjitabParameter> {
	if let Some(number) = parameter.strip_prefix("k")
		&& number.chars().all(|x| x.is_ascii_digit())
	{
		Some(JaKanjitabParameter::Alter(if number.is_empty() { 1 } else { number.parse::<u8>().unwrap() }))
	} else if let Some(number) = parameter.strip_prefix("o")
		&& number.chars().all(|x| x.is_ascii_digit())
	{
		Some(JaKanjitabParameter::Omit(if number.is_empty() { 1 } else { number.parse::<u8>().unwrap() }))
	} else if !parameter.is_empty() && parameter.chars().all(|x| x.is_ascii_digit()) {
		// NOTE: 龍卷.
		Some(JaKanjitabParameter::Reading(parameter.parse::<u8>().unwrap()))
	} else {
		None
	}
}

pub struct JaPos {
	pub readings: Vec<String>,
}

// Extract a list of readings from a part-of-speech template.
pub fn parse_ja_pos(is_generic: bool, arguments: &str) -> JaPos {
	let mut handled_pos = false;
	let mut readings = Vec::new();
	for parameter in TemplateParameters::new(arguments) {
		if let Some((parameter, value)) = parameter.split_once('=') {
			if parameter.chars().all(|x| x.is_ascii_digit()) {
				assert_eq!(parameter.parse::<u8>().unwrap() as usize, readings.len() + 1);
				readings.push(value.to_owned());
			}
		} else if is_generic && !handled_pos {
			handled_pos = true;
		} else {
			readings.push(parameter.into_owned());
		}
	}
	JaPos { readings }
}

pub fn parse_ja_altread(arguments: &str) -> JaPos {
	let mut readings = Vec::new();
	for parameter in TemplateParameters::new(arguments) {
		if let Some((parameter, value)) = parameter.split_once('=')
			&& parameter == "hira"
		{
			readings.push(value.to_owned());
		}
	}
	JaPos { readings }
}

pub struct JaPron {
	pub readings: Vec<String>,
	pub accents: Vec<JaPronAccent>,
	pub accent_locations: Vec<bool>,
}

pub fn parse_ja_pron(arguments: &str) -> Option<JaPron> {
	let mut readings = Vec::new();
	let mut accents = Vec::new();
	let mut accent_locations = Vec::new();

	for argument in TemplateParameters::new(arguments) {
		if let Some((parameter, value)) = argument.split_once('=') {
			match parse_ja_pron_parameter(parameter) {
				None => continue,
				Some(JaPronParameter::Reading(n)) => {
					assert_eq!(n as usize, readings.len() + 1);
					readings.push(value.to_owned());
				},
				Some(JaPronParameter::Accent(n)) => {
					let n = n.strict_sub(1) as usize;
					if n >= accents.len() {
						accents.resize(n + 1, JaPronAccent::None);
					}
					accents[n] = match value {
						"h" => JaPronAccent::Numeric(0),
						"a" => JaPronAccent::Numeric(1),
						"o" => JaPronAccent::Odaka,
						"" => JaPronAccent::None,
						n => JaPronAccent::Numeric(n.parse::<u8>().unwrap()),
					};
				},
				Some(JaPronParameter::Location(n)) => {
					let n = n.strict_sub(1) as usize;
					if n >= accent_locations.len() {
						accent_locations.resize(n + 1, false);
					}
					accent_locations[n] = true;
				},
			}
		} else {
			readings.push(argument.to_string());
		}
	}

	Some(JaPron { readings, accents, accent_locations })
}

#[derive(Clone)]
pub enum JaPronAccent {
	Numeric(u8),
	Odaka,
	None, // NOTE: 耀 has "acc=".
}

enum JaPronParameter {
	Reading(u8),
	Accent(u8),
	Location(u8),
}

fn parse_ja_pron_parameter(parameter: &str) -> Option<JaPronParameter> {
	if let Some(remainder) = parameter.strip_prefix("accent").or_else(|| parameter.strip_prefix("acc")) {
		let (number, tail) = cut(remainder, |c: char| !c.is_numeric());
		let number = if number.is_empty() { 1 } else { number.parse::<u8>().unwrap() };
		match tail {
			"" => Some(JaPronParameter::Accent(number)),
			"_ref" | "_note" => None,
			"_loc" => Some(JaPronParameter::Location(number)),
			_ => unimplemented!(),
		}
	} else if parameter.chars().all(|x| x.is_ascii_digit()) {
		Some(JaPronParameter::Reading(parameter.parse::<u8>().unwrap()))
	} else {
		None
	}
}

fn cut(text: &str, pattern: impl FnMut(char) -> bool) -> (&str, &str) {
	text.split_at(text.find(pattern).unwrap_or(text.len()))
}
