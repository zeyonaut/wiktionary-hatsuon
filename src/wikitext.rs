// Iterator over Wikitext templates {{...}} in a page.
pub struct FindTemplates<'a> {
	s: &'a str,
	char_indices: std::str::CharIndices<'a>,
	name: &'a str,
	start: usize,
	depth: usize,
	is_invalid: bool,
}

impl<'a> FindTemplates<'a> {
	pub fn new(s: &'a str) -> Self {
		Self { s, char_indices: s.char_indices(), name: "", start: 0, depth: 0, is_invalid: false }
	}
}

impl<'a> Iterator for FindTemplates<'a> {
	type Item = (&'a str, &'a str);

	fn next(&mut self) -> Option<Self::Item> {
		if self.is_invalid {
			return None;
		}

		'outer: while let Some((i, c)) = self.char_indices.next() {
			match c {
				c @ '{' if try_consume(&mut self.char_indices, &[c, c]) => {
					self.depth += 1;
				},
				c @ '}' if try_consume(&mut self.char_indices, &[c, c]) => {
					let Some(depth) = self.depth.checked_sub(1) else {
						self.is_invalid = true;
						return None;
					};
					self.depth = depth;
				},
				c @ '{' if try_consume(&mut self.char_indices, &[c]) => {
					if self.depth == 0 {
						// TODO: This is messy, but also likely incorrect. A more sound solution may be to
						//       integrate this parser with the parameter separator parser.
						let limit = self.s[i + 2..].find("}}");
						let Some(end) = limit.and_then(|x| self.s[i + 2..i + 2 + x].find('|')) else {
							let Some(end) = limit else {
								self.is_invalid = true;
								return None;
							};
							for _ in 0..self.s[i + 2..i + 2 + end].chars().count() + "}}".len() {
								self.char_indices.next();
							}
							let end = i + 2 + end;
							let name = &self.s[i + 2..end];
							self.start = end + 1;
							return Some((name, ""));
						};
						for _ in 0..self.s[i + 2..i + 2 + end].chars().count() + '|'.len_utf8() {
							self.char_indices.next();
						}
						let end = i + 2 + end;
						self.name = &self.s[i + 2..end];
						self.start = end + 1;
					}
					self.depth += 1;
				},
				c @ '}' if try_consume(&mut self.char_indices, &[c]) => {
					let Some(depth) = self.depth.checked_sub(1) else {
						self.is_invalid = true;
						return None;
					};
					self.depth = depth;
					if self.depth == 0 {
						return Some((self.name, &self.s[self.start..i]));
					}
				},
				'<' if try_consume(&mut self.char_indices, &['!', '-', '-']) => {
					while let Some((_, c)) = self.char_indices.next() {
						if c == '-' && try_consume(&mut self.char_indices, &['-', '>']) {
							continue 'outer;
						};
					}
					self.is_invalid = true;
					return None;
				},
				_ => (),
			}
		}

		None
	}
}

// Iterator over the parameters of a Wikitext template {{...|...|...}}.
pub struct TemplateParameters<'a> {
	s: &'a str,
	char_indices: std::str::CharIndices<'a>,
	start: usize,
	depth: usize,
	commentless: Option<String>,
	is_invalid: bool,
}

impl<'a> TemplateParameters<'a> {
	pub fn new(s: &'a str) -> Self {
		Self { s, char_indices: s.char_indices(), start: 0, depth: 0, commentless: None, is_invalid: false }
	}
}

impl<'a> Iterator for TemplateParameters<'a> {
	type Item = std::borrow::Cow<'a, str>;

	fn next(&mut self) -> Option<Self::Item> {
		if self.is_invalid {
			return None;
		}

		'outer: while let Some((i, c)) = self.char_indices.next() {
			match c {
				'|' if self.depth == 0 => {
					let part = &self.s[self.start..i];
					self.start = i + 1;
					if let Some(mut owned) = self.commentless.take() {
						owned.push_str(part);
						return Some(std::borrow::Cow::Owned(owned));
					} else {
						return Some(std::borrow::Cow::Borrowed(part));
					}
				},
				c @ '{' if try_consume(&mut self.char_indices, &[c, c]) => {
					self.depth += 1;
				},
				c @ '}' if try_consume(&mut self.char_indices, &[c, c]) => {
					let Some(depth) = self.depth.checked_sub(1) else {
						self.is_invalid = true;
						return None;
					};
					self.depth = depth;
				},
				c @ '[' | c @ '{' if try_consume(&mut self.char_indices, &[c]) => {
					self.depth += 1;
				},
				c @ ']' | c @ '}' if try_consume(&mut self.char_indices, &[c]) => {
					let Some(depth) = self.depth.checked_sub(1) else {
						self.is_invalid = true;
						return None;
					};
					self.depth = depth;
				},
				'<' if try_consume(&mut self.char_indices, &['!', '-', '-']) => {
					if let Some(has) = self.commentless.as_mut() {
						has.push_str(&self.s[self.start..i]);
					} else {
						self.commentless = Some(String::from(&self.s[self.start..i]));
					}
					while let Some((i, c)) = self.char_indices.next() {
						if c == '-' && try_consume(&mut self.char_indices, &['-', '>']) {
							self.start = i + 3;
							continue 'outer;
						};
					}
					self.is_invalid = true;
					return None;
				},
				_ => (),
			}
		}

		if self.start < self.s.len() || self.commentless.is_some() {
			let part = &self.s[self.start..];
			self.start = self.s.len();
			if let Some(mut owned) = self.commentless.take() {
				owned.push_str(part);
				return Some(std::borrow::Cow::Owned(owned));
			} else {
				return Some(std::borrow::Cow::Borrowed(part));
			}
		}

		None
	}
}

pub fn try_consume<'a>(chars: &mut std::str::CharIndices<'a>, peek: &[char]) -> bool {
	let mut cs = chars.clone().map(|a| a.1);
	for o in peek {
		if cs.next().is_none_or(|x| &x != o) {
			return false;
		}
	}
	for _ in 0..peek.len() {
		chars.next();
	}
	true
}

// Remove [[...]] and [[...|...]] from a string.
pub fn remove_links(reading: &str) -> String {
	let mut char_indices = reading.char_indices();
	let mut buffer = String::new();
	while let Some((i, c)) = char_indices.next() {
		if c == '[' && try_consume(&mut char_indices, &[c]) {
			let Some(x) = reading[i + 2..].find("]]") else { return buffer };
			let inner = &reading[i + 2..i + 2 + x];
			for _ in 0..inner.chars().count() + "]]".len() {
				char_indices.next();
			}
			if let Some((_, b)) = inner.split_once('|') {
				buffer.push_str(b);
			} else {
				buffer.push_str(inner);
			}
			continue;
		}

		buffer.push(c);
	}
	buffer
}
