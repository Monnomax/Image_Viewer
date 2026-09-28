pub fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let mut i = 0usize;
    let mut j = 0usize;

    while i < a_chars.len() && j < b_chars.len() {
        let a_char = a_chars[i];
        let b_char = b_chars[j];

        if a_char.is_ascii_digit() && b_char.is_ascii_digit() {
            let (a_num, a_next) = consume_number(&a_chars, i);
            let (b_num, b_next) = consume_number(&b_chars, j);

            match a_num.cmp(&b_num) {
                std::cmp::Ordering::Equal => {}
                ord => return ord,
            }

            if a_next != b_next {
                return a_next.cmp(&b_next);
            }

            i = a_next;
            j = b_next;
            continue;
        }

        let cmp = a_char.to_lowercase().to_string().cmp(&b_char.to_lowercase().to_string());
        if cmp != std::cmp::Ordering::Equal {
            return cmp;
        }

        i += 1;
        j += 1;
    }

    a_chars.len().cmp(&b_chars.len())
}

fn consume_number(chars: &[char], start: usize) -> (u64, usize) {
    let mut end = start;
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }
    let number: String = chars[start..end].iter().collect();
    (number.parse().unwrap_or(0), end)
}
