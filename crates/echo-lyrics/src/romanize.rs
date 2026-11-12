//! Offline Romanization for Korean (Hangul Revised Romanization) and Japanese (Hiragana / Katakana).

const INITIALS: [&str; 19] = [
    "g", "kk", "n", "d", "tt", "r", "m", "b", "pp", "s", "ss", "", "j", "jj", "ch", "k", "t", "p",
    "h",
];

const MEDIALS: [&str; 21] = [
    "a", "ae", "ya", "yae", "eo", "e", "yeo", "ye", "o", "wa", "wae", "oe", "yo", "u", "wo", "we",
    "wi", "yu", "eu", "ui", "i",
];

const FINALS: [&str; 28] = [
    "", "k", "k", "k", "n", "n", "n", "t", "l", "l", "l", "l", "l", "l", "l", "l", "m", "p", "p",
    "t", "t", "ng", "t", "t", "k", "t", "p", "t",
];

pub fn contains_cjk_or_hangul_or_kana(s: &str) -> bool {
    s.chars().any(|c| {
        ('\u{AC00}'..='\u{D7A3}').contains(&c)
            || ('\u{3040}'..='\u{309F}').contains(&c)
            || ('\u{30A0}'..='\u{30FF}').contains(&c)
    })
}

pub fn romanize_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        let ch = chars[i];
        let code = ch as u32;
        if (0xAC00..=0xD7A3).contains(&code) {
            let idx = (code - 0xAC00) as usize;
            let ini = idx / (21 * 28);
            let med = (idx % (21 * 28)) / 28;
            let fin = idx % 28;
            out.push_str(INITIALS[ini]);
            out.push_str(MEDIALS[med]);
            out.push_str(FINALS[fin]);
            i += 1;
            continue;
        }

        // Convert Katakana to Hiragana for unified kana lookup
        let kana = if ('\u{30A1}'..='\u{30F6}').contains(&ch) {
            char::from_u32(ch as u32 - 0x60).unwrap_or(ch)
        } else {
            ch
        };

        if kana == 'ー' {
            if let Some(last) = out.chars().last() {
                if matches!(last, 'a' | 'i' | 'u' | 'e' | 'o') {
                    out.push(last);
                }
            }
            i += 1;
            continue;
        }

        if kana == 'っ' {
            if let Some(&next_raw) = chars.get(i + 1) {
                let next_kana = if ('\u{30A1}'..='\u{30F6}').contains(&next_raw) {
                    char::from_u32(next_raw as u32 - 0x60).unwrap_or(next_raw)
                } else {
                    next_raw
                };
                if let Some(rom) = kana_to_romaji(next_kana) {
                    if let Some(first) = rom.chars().next() {
                        out.push(first);
                    }
                }
            }
            i += 1;
            continue;
        }

        if let Some(next_raw) = chars.get(i + 1).copied() {
            let next_kana = if ('\u{30A1}'..='\u{30F6}').contains(&next_raw) {
                char::from_u32(next_raw as u32 - 0x60).unwrap_or(next_raw)
            } else {
                next_raw
            };
            if let Some(digraph) = kana_digraph(kana, next_kana) {
                out.push_str(digraph);
                i += 2;
                continue;
            }
        }

        if let Some(rom) = kana_to_romaji(kana) {
            out.push_str(rom);
        } else {
            out.push(ch);
        }
        i += 1;
    }

    out
}

fn kana_digraph(first: char, second: char) -> Option<&'static str> {
    let base = match first {
        'き' => "ky",
        'ぎ' => "gy",
        'し' => "sh",
        'じ' => "j",
        'ち' => "ch",
        'に' => "ny",
        'ひ' => "hy",
        'び' => "by",
        'ぴ' => "py",
        'み' => "my",
        'り' => "ry",
        _ => return None,
    };
    let tail = match second {
        'ゃ' => "a",
        'ゅ' => "u",
        'ょ' => "o",
        _ => return None,
    };
    Some(match (base, tail) {
        ("ky", "a") => "kya",
        ("ky", "u") => "kyu",
        ("ky", "o") => "kyo",
        ("gy", "a") => "gya",
        ("gy", "u") => "gyu",
        ("gy", "o") => "gyo",
        ("sh", "a") => "sha",
        ("sh", "u") => "shu",
        ("sh", "o") => "sho",
        ("j", "a") => "ja",
        ("j", "u") => "ju",
        ("j", "o") => "jo",
        ("ch", "a") => "cha",
        ("ch", "u") => "chu",
        ("ch", "o") => "cho",
        ("ny", "a") => "nya",
        ("ny", "u") => "nyu",
        ("ny", "o") => "nyo",
        ("hy", "a") => "hya",
        ("hy", "u") => "hyu",
        ("hy", "o") => "hyo",
        ("by", "a") => "bya",
        ("by", "u") => "byu",
        ("by", "o") => "byo",
        ("py", "a") => "pya",
        ("py", "u") => "pyu",
        ("py", "o") => "pyo",
        ("my", "a") => "mya",
        ("my", "u") => "myu",
        ("my", "o") => "myo",
        ("ry", "a") => "rya",
        ("ry", "u") => "ryu",
        ("ry", "o") => "ryo",
        _ => return None,
    })
}

fn kana_to_romaji(ch: char) -> Option<&'static str> {
    Some(match ch {
        'あ' | 'ぁ' => "a",
        'い' | 'ぃ' => "i",
        'う' | 'ぅ' => "u",
        'え' | 'ぇ' => "e",
        'お' | 'ぉ' => "o",
        'か' => "ka",
        'き' => "ki",
        'く' => "ku",
        'け' => "ke",
        'こ' => "ko",
        'が' => "ga",
        'ぎ' => "gi",
        'ぐ' => "gu",
        'げ' => "ge",
        'ご' => "go",
        'さ' => "sa",
        'し' => "shi",
        'す' => "su",
        'せ' => "se",
        'そ' => "so",
        'ざ' => "za",
        'じ' => "ji",
        'ず' => "zu",
        'ぜ' => "ze",
        'ぞ' => "zo",
        'た' => "ta",
        'ち' => "chi",
        'つ' => "tsu",
        'て' => "te",
        'と' => "to",
        'だ' => "da",
        'ぢ' => "ji",
        'づ' => "zu",
        'で' => "de",
        'ど' => "do",
        'な' => "na",
        'に' => "ni",
        'ぬ' => "nu",
        'ね' => "ne",
        'の' => "no",
        'は' => "ha",
        'ひ' => "hi",
        'ふ' => "fu",
        'へ' => "he",
        'ほ' => "ho",
        'ば' => "ba",
        'び' => "bi",
        'ぶ' => "bu",
        'べ' => "be",
        'ぼ' => "bo",
        'ぱ' => "pa",
        'ぴ' => "pi",
        'ぷ' => "pu",
        'ぺ' => "pe",
        'ぽ' => "po",
        'ま' => "ma",
        'み' => "mi",
        'む' => "mu",
        'め' => "me",
        'も' => "mo",
        'や' | 'ゃ' => "ya",
        'ゆ' | 'ゅ' => "yu",
        'よ' | 'ょ' => "yo",
        'ら' => "ra",
        'り' => "ri",
        'る' => "ru",
        'れ' => "re",
        'ろ' => "ro",
        'わ' | 'ゎ' => "wa",
        'を' => "wo",
        'ん' => "n",
        '・' => " ",
        _ => return None,
    })
}

