
use crate::my_dbg;

/// Given an input str, a start pattern & an end pattern extracts & returns the str, if any,
/// between the two.
/// If either the start pattern or the end pattern  is not found, returns an empty str.
/// If the return pattern starts with "\<p>" & ends with "\</p>", these are removed.
pub fn extract<'a>(input_string: &'a str, start: &str, end: &str) -> &'a str {
    // the consistent use of the lifetime specifier 'a means that they must all have the same lifetime.

    let Some(position_start) = input_string.find(start) else {
        return "could not find start";
    };

    let Some(position_end) = input_string[position_start + start.len()..].find(end) else {
        return "could not find end\r";
    };

    let return_value =
        &input_string[position_start + start.len()..position_start + start.len() + position_end];

    if return_value.starts_with("<p>") && return_value.ends_with("</p>") {
        &return_value[3..return_value.len() - 4]
    } else {
        return_value
    }
}
