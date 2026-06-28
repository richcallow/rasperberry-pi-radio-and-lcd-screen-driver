/// Given an input str, a start pattern & an end pattern extracts & returns the str, if any,
/// between the two.
/// If either the start pattern or the end pattern  is not found, returns an empty str.
/// If the return pattern starts with "\<p>" & ends with "\</p>", these are removed.
pub fn extract<'a>(input_str: &'a str, start: &str, end: &str) -> &'a str {
    // the consistent use of the lifetime specifier 'a means that they must all have the same lifetime.

    // we cannot use strip_suffix as we remove all after the end string, which is variable & unknown

    let Some(position_start) = input_str.find(start) else {
        return "";
    };

    let Some(position_end) = input_str[position_start + start.len()..].find(end) else {
        return "";
    };

    let return_value =
        &input_str[position_start + start.len()..position_start + start.len() + position_end];

    if return_value.starts_with("<p>") && return_value.ends_with("</p>") {
        &return_value[3..return_value.len() - 4]
    } else {
        return_value
    }
}
