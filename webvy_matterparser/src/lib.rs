use toml::{Table, Value};

#[derive(Debug)]
pub struct Parser {
    delimiter: String,
    excerpt: Option<String>,
}

impl Parser {
    pub fn new(delimiter: impl Into<String>) -> Self {
        Self {
            delimiter: delimiter.into(),
            excerpt: None,
        }
    }

    pub fn with_excerpt(mut self, excerpt: impl Into<String>) -> Self {
        self.excerpt = Some(excerpt.into());
        self
    }

    pub fn parse(&self, page: &str) -> Option<ParsedData> {
        page.strip_prefix(self.delimiter.as_str())
            .map(|matter| matter.split_terminator(self.delimiter.as_str()))
            .map(|mut split_text| {
                let matter = split_text
                    .next()
                    .and_then(|matter| toml::from_str(matter).ok());

                let content = split_text.next();

                let (excerpt, content) = content
                    .zip(self.excerpt.as_ref())
                    .and_then(|(text, delimiter)| text.split_once(delimiter))
                    .map_or_else(
                        || {
                            (
                                None,
                                content.map_or_else(
                                    || page.trim().to_string(),
                                    |content| content.trim().to_string(),
                                ),
                            )
                        },
                        |(excerpt, content)| {
                            (Some(excerpt.trim().to_string()), content.trim().to_string())
                        },
                    );

                ParsedData {
                    matter,
                    excerpt,
                    content,
                }
            })
    }
}

#[derive(Debug)]
pub struct ParsedData {
    matter: Option<Table>,
    excerpt: Option<String>,
    content: String,
}

impl ParsedData {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.matter.as_ref().and_then(|table| table.get(key))
    }

    pub fn excerpt(&self) -> Option<&str> {
        self.excerpt.as_deref()
    }

    pub fn content(&self) -> &str {
        self.content.as_str()
    }

    pub fn take_excerpt(&mut self) -> Option<String> {
        self.excerpt.take()
    }

    pub fn take_matter(&mut self) -> Option<Table> {
        self.matter.take()
    }

    pub fn take_content(&mut self) -> String {
        self.content.drain(..).collect()
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new("+++")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_table_from_front_matter() {
        let test_toml = "+++\n[thing]\nkey = true+++\nOther text";

        let result = Parser::default().parse(test_toml).unwrap();

        let expected = toml::from_str("key = true").unwrap();

        assert_eq!(result.get("thing"), Some(&expected));
    }

    #[test]
    fn extract_all_data_from_page() {
        let parser = Parser::default().with_excerpt("<!-- excerpt -->");

        let test_page = "+++\n[thing]\nkey = true+++\nOther text\n<!-- excerpt -->\nEverything else I don't want to include.\n\nA Paragraph\n";

        let expected_toml = toml::from_str("key = true").unwrap();
        let expected_excerpt = Some(String::from("Other text"));

        let result = parser.parse(test_page).unwrap();

        assert_eq!(result.matter.unwrap().get("thing"), Some(&expected_toml));
        assert_eq!(result.excerpt, expected_excerpt);
        assert_eq!(
            result.content,
            "Everything else I don't want to include.\n\nA Paragraph"
        );
    }

    #[test]
    fn returns_none_if_unable_to_find_frontmatter() {
        let parser = Parser::default();

        let test_page = "Everything else I don't want to include.\n\nA Paragraph\n";

        let result = parser.parse(test_page);

        assert!(result.is_none());
    }
}
