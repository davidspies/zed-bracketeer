use zed_extension_api::{
    self as zed, EditorCommandContext, EditorCommandResult, EditorEdit, EditorSelection, Range,
};

const LINE_TOLERANCE: usize = 8;

struct Bracketeer;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Pair {
    open: char,
    close: char,
}

#[derive(Clone, Copy)]
struct DelimiterMatch {
    open: usize,
    close: usize,
    pair: Pair,
    selection: EditorSelection,
}

impl zed::Extension for Bracketeer {
    fn new() -> Self {
        Self
    }

    fn run_editor_command(
        &mut self,
        command_id: String,
        context: EditorCommandContext,
    ) -> zed::Result<Option<EditorCommandResult>> {
        let result = match command_id.as_str() {
            "bracketeer.swapBrackets" => replace_brackets(&context, Replacement::Cycle)?,
            "bracketeer.removeBrackets" => replace_brackets(&context, Replacement::Remove)?,
            "bracketeer.selectBracketContent" => select_bracket_content(&context)?,
            "bracketeer.changeBracketsTo.parentheses" => {
                replace_brackets(&context, Replacement::Pair(Pair::new('(', ')')))?
            }
            "bracketeer.changeBracketsTo.square" => {
                replace_brackets(&context, Replacement::Pair(Pair::new('[', ']')))?
            }
            "bracketeer.changeBracketsTo.curly" => {
                replace_brackets(&context, Replacement::Pair(Pair::new('{', '}')))?
            }
            "bracketeer.changeBracketsTo.angle" => {
                replace_brackets(&context, Replacement::Pair(Pair::new('<', '>')))?
            }
            "bracketeer.swapQuotes" => replace_quotes(&context, Replacement::Cycle)?,
            "bracketeer.removeQuotes" => replace_quotes(&context, Replacement::Remove)?,
            "bracketeer.selectQuotesContent" => select_quote_content(&context)?,
            "bracketeer.changeQuotesTo.single" => {
                replace_quotes(&context, Replacement::Pair(Pair::same('\'')))?
            }
            "bracketeer.changeQuotesTo.double" => {
                replace_quotes(&context, Replacement::Pair(Pair::same('"')))?
            }
            "bracketeer.changeQuotesTo.backtick" => {
                replace_quotes(&context, Replacement::Pair(Pair::same('`')))?
            }
            _ => return Ok(None),
        };

        Ok(result)
    }
}

zed::register_extension!(Bracketeer);

#[derive(Clone, Copy)]
enum Replacement {
    Cycle,
    Remove,
    Pair(Pair),
}

impl Pair {
    const fn new(open: char, close: char) -> Self {
        Self { open, close }
    }

    const fn same(quote: char) -> Self {
        Self {
            open: quote,
            close: quote,
        }
    }
}

fn replace_brackets(
    context: &EditorCommandContext,
    replacement: Replacement,
) -> zed::Result<Option<EditorCommandResult>> {
    let pairs = bracket_pairs(context);
    replace_matches(parse_brackets(context, &pairs), &pairs, replacement)
}

fn replace_quotes(
    context: &EditorCommandContext,
    replacement: Replacement,
) -> zed::Result<Option<EditorCommandResult>> {
    let pairs = quote_pairs(context);
    replace_matches(parse_quotes(context, &pairs), &pairs, replacement)
}

fn replace_matches(
    matches: Vec<DelimiterMatch>,
    pairs: &[Pair],
    replacement: Replacement,
) -> zed::Result<Option<EditorCommandResult>> {
    if matches.is_empty() {
        return Ok(None);
    }

    let mut edits = Vec::with_capacity(matches.len() * 2);
    let selections = matches
        .iter()
        .map(|delimiter_match| delimiter_match.selection)
        .collect::<Vec<_>>();

    for delimiter_match in matches {
        let pair = match replacement {
            Replacement::Cycle => cycle_pair(delimiter_match.pair, pairs)
                .ok_or_else(|| "Unable to cycle delimiter pair".to_string())?,
            Replacement::Remove => Pair::same('\0'),
            Replacement::Pair(pair) => pair,
        };

        let (open, close) = if matches!(replacement, Replacement::Remove) {
            (String::new(), String::new())
        } else {
            (pair.open.to_string(), pair.close.to_string())
        };

        edits.push(edit_for_char(
            delimiter_match.open,
            delimiter_match.pair.open,
            open,
        )?);
        edits.push(edit_for_char(
            delimiter_match.close,
            delimiter_match.pair.close,
            close,
        )?);
    }

    let selections = remap_selections(&selections, &edits)?;

    Ok(Some(EditorCommandResult {
        edits,
        selections: Some(selections),
    }))
}

fn select_bracket_content(
    context: &EditorCommandContext,
) -> zed::Result<Option<EditorCommandResult>> {
    select_content(parse_brackets(context, &bracket_pairs(context)))
}

fn select_quote_content(
    context: &EditorCommandContext,
) -> zed::Result<Option<EditorCommandResult>> {
    select_content(parse_quotes(context, &quote_pairs(context)))
}

fn select_content(matches: Vec<DelimiterMatch>) -> zed::Result<Option<EditorCommandResult>> {
    if matches.is_empty() {
        return Ok(None);
    }

    let selections = matches
        .into_iter()
        .map(|delimiter_match| {
            let content_start = delimiter_match.open + delimiter_match.pair.open.len_utf8();
            let content_end = delimiter_match.close;
            if delimiter_match.selection.start as usize == content_start
                && delimiter_match.selection.end as usize == content_end
            {
                EditorSelection {
                    start: delimiter_match.open as u64,
                    end: (delimiter_match.close + delimiter_match.pair.close.len_utf8()) as u64,
                    reversed: delimiter_match.selection.reversed,
                }
            } else {
                EditorSelection {
                    start: content_start as u64,
                    end: content_end as u64,
                    reversed: delimiter_match.selection.reversed,
                }
            }
        })
        .collect();

    Ok(Some(EditorCommandResult {
        edits: Vec::new(),
        selections: Some(selections),
    }))
}

fn parse_brackets(context: &EditorCommandContext, pairs: &[Pair]) -> Vec<DelimiterMatch> {
    context
        .selections
        .iter()
        .filter_map(|selection| find_brackets_around_selection(&context.text, *selection, pairs))
        .collect()
}

fn parse_quotes(context: &EditorCommandContext, pairs: &[Pair]) -> Vec<DelimiterMatch> {
    context
        .selections
        .iter()
        .filter_map(|selection| {
            find_quotes_around_selection(&context.text, *selection, pairs, LINE_TOLERANCE)
        })
        .collect()
}

fn find_brackets_around_selection(
    text: &str,
    selection: EditorSelection,
    pairs: &[Pair],
) -> Option<DelimiterMatch> {
    let (before_end, after_start) = selection_bounds_for_brackets(text, selection)?;
    let (open, pair) = find_opening_bracket(text, before_end, pairs)?;
    let close = find_closing_bracket(text, after_start, pair)?;
    Some(DelimiterMatch {
        open,
        close,
        pair,
        selection,
    })
}

fn find_opening_bracket(text: &str, before_end: usize, pairs: &[Pair]) -> Option<(usize, Pair)> {
    let mut stack: Vec<char> = Vec::new();
    for (index, character) in text[..before_end].char_indices().rev() {
        if let Some(pair) = pairs.iter().copied().find(|pair| pair.close == character) {
            stack.push(pair.open);
        } else if let Some(pair) = pairs.iter().copied().find(|pair| pair.open == character) {
            if stack.last().copied() == Some(character) {
                stack.pop();
            } else {
                return Some((index, pair));
            }
        }
    }
    None
}

fn find_closing_bracket(text: &str, after_start: usize, target_pair: Pair) -> Option<usize> {
    let mut depth = 0usize;
    for (relative_index, character) in text[after_start..].char_indices() {
        if character == target_pair.open {
            depth += 1;
        } else if character == target_pair.close {
            if depth == 0 {
                return Some(after_start + relative_index);
            }
            depth -= 1;
        }
    }
    None
}

fn selection_bounds_for_brackets(text: &str, selection: EditorSelection) -> Option<(usize, usize)> {
    let start = usize::try_from(selection.start).ok()?;
    let end = usize::try_from(selection.end).ok()?;
    if start > end || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return None;
    }

    if start == end {
        let word_start = previous_word_boundary(text, start);
        let word_end = next_word_boundary(text, start);
        Some((word_start, word_end))
    } else {
        Some((
            previous_word_boundary(text, start),
            next_word_boundary(text, end),
        ))
    }
}

fn previous_word_boundary(text: &str, offset: usize) -> usize {
    let mut boundary = offset;
    for (index, character) in text[..offset].char_indices().rev() {
        if !is_word_character(character) {
            break;
        }
        boundary = index;
    }
    boundary
}

fn next_word_boundary(text: &str, offset: usize) -> usize {
    for (relative_index, character) in text[offset..].char_indices() {
        if !is_word_character(character) {
            return offset + relative_index;
        }
    }
    text.len()
}

fn is_word_character(character: char) -> bool {
    character == '_' || character.is_alphanumeric()
}

fn find_quotes_around_selection(
    text: &str,
    selection: EditorSelection,
    pairs: &[Pair],
    line_tolerance: usize,
) -> Option<DelimiterMatch> {
    let start = usize::try_from(selection.start).ok()?;
    let end = usize::try_from(selection.end).ok()?;
    if start > end || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return None;
    }

    let search_range = line_tolerant_range(text, start, end, line_tolerance)?;
    find_quote_in_range(text, selection, pairs, search_range)
        .or_else(|| find_quote_in_range(text, selection, pairs, 0..text.len()))
}

fn find_quote_in_range(
    text: &str,
    selection: EditorSelection,
    pairs: &[Pair],
    range: std::ops::Range<usize>,
) -> Option<DelimiterMatch> {
    let cursor = usize::try_from(selection.start).ok()?;
    let mut best_match = None;

    for pair in pairs {
        if pair.open != pair.close {
            continue;
        }

        let mut open = None;
        let mut escaped = false;
        for (relative_index, character) in text[range.clone()].char_indices() {
            let index = range.start + relative_index;
            if escaped {
                escaped = false;
                continue;
            }
            if character == '\\' {
                escaped = true;
                continue;
            }
            if character != pair.open {
                continue;
            }

            match open {
                Some(open_index) => {
                    if cursor > open_index && cursor < index + character.len_utf8() {
                        let candidate = DelimiterMatch {
                            open: open_index,
                            close: index,
                            pair: *pair,
                            selection,
                        };
                        if best_match.as_ref().is_none_or(|current: &DelimiterMatch| {
                            candidate.open >= current.open && candidate.close <= current.close
                        }) {
                            best_match = Some(candidate);
                        }
                    }
                    open = None;
                }
                None => open = Some(index),
            }
        }
    }

    best_match
}

fn line_tolerant_range(
    text: &str,
    start: usize,
    end: usize,
    line_tolerance: usize,
) -> Option<std::ops::Range<usize>> {
    let selection_line_start = line_start(text, start);
    let selection_line_end = line_end(text, end);
    let mut range_start = selection_line_start;
    let mut range_end = selection_line_end;

    for _ in 0..line_tolerance {
        if range_start == 0 {
            break;
        }
        range_start = line_start(text, range_start.saturating_sub(1));
    }

    for _ in 0..line_tolerance {
        if range_end >= text.len() {
            break;
        }
        range_end = line_end(text, range_end + 1);
    }

    if text.is_char_boundary(range_start) && text.is_char_boundary(range_end) {
        Some(range_start..range_end)
    } else {
        None
    }
}

fn line_start(text: &str, offset: usize) -> usize {
    text[..offset].rfind('\n').map_or(0, |index| index + 1)
}

fn line_end(text: &str, offset: usize) -> usize {
    text[offset..]
        .find('\n')
        .map_or(text.len(), |index| offset + index)
}

fn edit_for_char(offset: usize, original: char, new_text: String) -> zed::Result<EditorEdit> {
    let end = offset
        .checked_add(original.len_utf8())
        .ok_or_else(|| "Delimiter range overflowed".to_string())?;

    Ok(EditorEdit {
        range: Range {
            start: u32::try_from(offset)
                .map_err(|_| "Delimiter range start exceeded u32".to_string())?,
            end: u32::try_from(end).map_err(|_| "Delimiter range end exceeded u32".to_string())?,
        },
        new_text,
    })
}

fn remap_selections(
    selections: &[EditorSelection],
    edits: &[EditorEdit],
) -> zed::Result<Vec<EditorSelection>> {
    selections
        .iter()
        .map(|selection| {
            Ok(EditorSelection {
                start: remap_offset(selection.start, edits)?,
                end: remap_offset(selection.end, edits)?,
                reversed: selection.reversed,
            })
        })
        .collect()
}

fn remap_offset(offset: u64, edits: &[EditorEdit]) -> zed::Result<u64> {
    let mut edits = edits.iter().collect::<Vec<_>>();
    edits.sort_by_key(|edit| edit.range.start);

    let mut delta = 0i64;
    for edit in edits {
        let edit_start = u64::from(edit.range.start);
        let edit_end = u64::from(edit.range.end);
        if offset < edit_start {
            break;
        }

        let old_len = edit_end
            .checked_sub(edit_start)
            .ok_or_else(|| "Edit range end preceded start".to_string())?;
        let new_len = u64::try_from(edit.new_text.len())
            .map_err(|_| "Edit replacement length exceeded u64".to_string())?;

        if offset == edit_start {
            return apply_delta(edit_start, delta);
        }

        if offset < edit_end {
            let mapped = edit_start
                .checked_add(new_len)
                .ok_or_else(|| "Mapped selection offset overflowed".to_string())?;
            return apply_delta(mapped, delta);
        }

        delta = delta
            .checked_add(
                i64::try_from(new_len)
                    .map_err(|_| "Edit replacement length exceeded i64".to_string())?
                    - i64::try_from(old_len)
                        .map_err(|_| "Edit range length exceeded i64".to_string())?,
            )
            .ok_or_else(|| "Selection offset delta overflowed".to_string())?;
    }

    apply_delta(offset, delta)
}

fn apply_delta(offset: u64, delta: i64) -> zed::Result<u64> {
    if delta >= 0 {
        offset
            .checked_add(delta as u64)
            .ok_or_else(|| "Mapped selection offset overflowed".to_string())
    } else {
        offset
            .checked_sub(delta.unsigned_abs())
            .ok_or_else(|| "Mapped selection offset underflowed".to_string())
    }
}

fn cycle_pair(current: Pair, pairs: &[Pair]) -> Option<Pair> {
    let index = pairs.iter().position(|pair| *pair == current)?;
    Some(pairs[(index + 1) % pairs.len()])
}

fn bracket_pairs(context: &EditorCommandContext) -> Vec<Pair> {
    match normalized_language(context).as_deref() {
        Some("json") => vec![Pair::new('[', ']'), Pair::new('{', '}')],
        Some("css") | Some("html") => vec![Pair::new('(', ')'), Pair::new('{', '}')],
        Some("typescript") | Some("typescriptreact") | Some("tsx") => vec![
            Pair::new('(', ')'),
            Pair::new('[', ']'),
            Pair::new('{', '}'),
            Pair::new('<', '>'),
        ],
        _ => vec![
            Pair::new('(', ')'),
            Pair::new('[', ']'),
            Pair::new('{', '}'),
        ],
    }
}

fn quote_pairs(context: &EditorCommandContext) -> Vec<Pair> {
    match normalized_language(context).as_deref() {
        Some("javascript") | Some("typescript") | Some("typescriptreact") | Some("tsx") => {
            vec![Pair::same('\''), Pair::same('"'), Pair::same('`')]
        }
        Some("json") => vec![Pair::same('"')],
        _ => vec![Pair::same('"'), Pair::same('\'')],
    }
}

fn normalized_language(context: &EditorCommandContext) -> Option<String> {
    context
        .language
        .as_ref()
        .map(|language| language.to_lowercase().replace([' ', '-'], ""))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(
        text: &str,
        selection: std::ops::Range<usize>,
        language: &str,
    ) -> EditorCommandContext {
        EditorCommandContext {
            text: text.to_string(),
            selections: vec![EditorSelection {
                start: selection.start as u64,
                end: selection.end as u64,
                reversed: false,
            }],
            language: Some(language.to_string()),
            path: None,
        }
    }

    fn apply(text: &str, result: EditorCommandResult) -> String {
        let mut output = text.to_string();
        let mut edits = result.edits;
        edits.sort_by_key(|edit| edit.range.start);
        for edit in edits.into_iter().rev() {
            output.replace_range(
                edit.range.start as usize..edit.range.end as usize,
                &edit.new_text,
            );
        }
        output
    }

    #[test]
    fn swaps_nested_brackets() {
        let context = context("foo(bar[baz])", 8..8, "JavaScript");
        let result = replace_brackets(&context, Replacement::Cycle)
            .unwrap()
            .unwrap();
        assert_eq!(apply(&context.text, result), "foo(bar{baz})");
    }

    #[test]
    fn preserves_cursor_between_empty_brackets_when_swapping() {
        let context = context("()", 1..1, "JavaScript");
        let result = replace_brackets(&context, Replacement::Cycle)
            .unwrap()
            .unwrap();
        let selections = result.selections.clone().unwrap();

        assert_eq!(apply(&context.text, result), "[]");
        assert_eq!(selections[0].start, 1);
        assert_eq!(selections[0].end, 1);
    }

    #[test]
    fn removes_surrounding_brackets() {
        let context = context("foo(bar[baz])", 4..12, "JavaScript");
        let result = replace_brackets(&context, Replacement::Remove)
            .unwrap()
            .unwrap();
        assert_eq!(apply(&context.text, result), "foobar[baz]");
    }

    #[test]
    fn remaps_cursor_when_removing_brackets() {
        let context = context("(alpha)", 3..3, "JavaScript");
        let result = replace_brackets(&context, Replacement::Remove)
            .unwrap()
            .unwrap();
        let selections = result.selections.clone().unwrap();

        assert_eq!(apply(&context.text, result), "alpha");
        assert_eq!(selections[0].start, 2);
        assert_eq!(selections[0].end, 2);
    }

    #[test]
    fn selects_then_expands_bracket_content() {
        let first_context = context("(alpha)", 2..2, "JavaScript");
        let result = select_bracket_content(&first_context).unwrap().unwrap();
        let selections = result.selections.unwrap();
        assert_eq!(selections[0].start, 1);
        assert_eq!(selections[0].end, 6);

        let second_context = context("(alpha)", 1..6, "JavaScript");
        let result = select_bracket_content(&second_context).unwrap().unwrap();
        let selections = result.selections.unwrap();
        assert_eq!(selections[0].start, 0);
        assert_eq!(selections[0].end, 7);
    }

    #[test]
    fn swaps_quotes() {
        let context = context("const x = 'value';", 12..12, "JavaScript");
        let result = replace_quotes(&context, Replacement::Cycle)
            .unwrap()
            .unwrap();
        assert_eq!(apply(&context.text, result), "const x = \"value\";");
    }
}
