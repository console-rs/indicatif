use std::borrow::Cow;

use regex::{Regex, Captures};


pub fn expand_template<'a, F: Fn(&str) -> String>(s: &'a str, f: F) -> Cow<'a, str> {
    lazy_static! {
        static ref VAR_RE: Regex = Regex::new(
            r"(\}\})|\{(\{|[^}]+\})").unwrap();
    }
    VAR_RE.replace_all(s, |caps: &Captures| {
        if caps.get(1).is_some() {
            "}".into()
        } else {
            let key = &caps[2];
            if key == "{" {
                "{".into()
            } else {
                f(&key[..key.len() - 1])
            }
        }
    })
}

#[test]
fn test_expand_template() {
    let rv = expand_template("{{ {foo} {bar} }}", |key| {
        key.to_uppercase()
    });
    assert_eq!(&rv, "{ FOO BAR }");
}
