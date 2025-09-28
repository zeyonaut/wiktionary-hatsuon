#!/usr/bin/env -S cargo +nightly -Zscript
---cargo
[dependencies]
xml = "1.0"

[profile.dev]
opt-level = 3
debug-assertions = false
---

fn main() {
	let args: Vec<_> = std::env::args().collect();
	let directory = std::env::current_dir().unwrap().join(&args[1]);
	let mut files = Vec::new();
	for entry in std::fs::read_dir(&directory).unwrap().map(Result::unwrap) {
		let path = entry.path();
		if path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.starts_with("xml")) {
			files.push(path);
		}
	}

	let output_path = directory.join("enwiktionary.bin");
	let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(output_path).unwrap();

	for file in files {
		let input = std::io::BufReader::new(std::fs::File::open(file).unwrap());
		let mut parser = xml::reader::EventReader::new(input);
		let mut title = None;
		let mut title_text: Vec<(String, String)> = Vec::new();
		loop {
			match parser.next() {
				Ok(xml::reader::XmlEvent::StartElement { name, .. }) => {
					if name.local_name == "title"
						&& let Ok(xml::reader::XmlEvent::Characters(s)) = parser.next()
					{
						title = Some(s);
					} else if name.local_name == "text"
						&& let Ok(xml::reader::XmlEvent::Characters(s)) = parser.next()
						&& title.as_ref().is_some_and(|title| {
							!(title.starts_with("User:")
								|| title.starts_with("Wiktionary:")
								|| title.starts_with("User talk:")
								|| title.starts_with("Appendix:"))
								&& s.contains("==Japanese==\n")
						}) {
						title_text.push((title.take().unwrap(), s));
					};
				},
				Ok(xml::reader::XmlEvent::EndDocument) => break,
				Err(e) => panic!("{e}"),
				_ => {},
			}
		}
		for (title, text) in title_text {
			use std::io::Write as _;
			let title = title.as_bytes();
			let text = text.as_bytes();
			output.write_all(&(title.len() as u64).to_le_bytes()).unwrap();
			output.write_all(title).unwrap();
			output.write_all(&(text.len() as u64).to_le_bytes()).unwrap();
			output.write_all(text).unwrap();
		}
	}
}
