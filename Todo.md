# Todo
- replace `"string".try_into().unwrap()` with `Text::from("string").unwrap()` or `Text::new_or_panic("string")`
- fix `fuzz::damaged` test, as it says `memory allocation of 590558243200 bytes failed`