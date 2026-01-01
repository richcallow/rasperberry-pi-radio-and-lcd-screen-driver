use crate::my_dbg;

/// Given an input str, a start pattern & an end pattern extracts & returns the str, if any,
/// between the two.
/// If either the start pattern or the end pattern  is not found, returns an empty str.
/// If the return pattern starts with "\<p>" & ends with "\</p>", these are removed.
pub fn extractz<'a>(input_string: &'a str, start: &str, end: &str) -> &'a str {
    // the consistent use of the lifetime specifier 'a means that they must all have the same lifetime.

    let Some(position_start) = input_string.find(start) else {
        return "could_not find start";
    };

    let Some(position_end) = input_string[position_start + start.len()..].find(end) else {
        return "could not find end\r";
    };

    //my_dbg!(position_start, position_start + start.len(), start.len(), position_end , start);

    let return_value = &input_string[position_start + start.len()..position_start + start.len() + position_end];

    if return_value.starts_with("<p>") && return_value.ends_with("</p>") {
        &return_value[3..return_value.len() - 4]
    } else {
        return_value
    }
}

/// Given an input str, a start pattern & an end pattern extracts & returns the str, if any,
/// between the two.
/// If either the start pattern or the end pattern  is not found, returns an empty str.
/// If the return pattern starts with "\<p>" & ends with "\</p>", these are removed.
pub fn extract<'a>(input_string: &'a str, start: &'a str, end: &'a str) -> &'a str {
    // the consisten use of the lifetime specifier 'a means that they must all have the same lifetime.
    let mut return_value;
    if let Some(position_start) = input_string.find(start) {
        if let Some(position_end) = input_string.find(end)
            && position_start < position_end - start.len()
        {
            return_value = &input_string[position_start + start.len()..position_end];
        } else {
            my_dbg!(position_start, input_string.find(end));
            return "end too soon";
        }
    } else {
        return "start not found";
    };
    if return_value.starts_with("<p>") && return_value.ends_with("</p>") {
        return_value = &return_value[3..return_value.len() - 4]
    }
    return_value
}
