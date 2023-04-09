use std::borrow::Cow;

use regex::Regex;

pub fn str_regex_replace<'a>(from: &str, data: &'a str, to: &str) -> Cow<'a, str> {
    let regex = format!(r"\b{}\b", &from);
    let re = Regex::new(regex.as_str()).unwrap();
    re.replace_all(data, &*to)
}

pub fn str_regex_matches(from: &str, data: &str) -> usize {
    let regex = format!(r"\b{}\b", &from);
    let re = Regex::new(regex.as_str()).unwrap();

    re.find_iter(data).count()
}
