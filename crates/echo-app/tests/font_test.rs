#[test]
fn test_multilingual_coverage() {
    use iced::advanced::graphics::text::font_system;
    let mut fs_lock = font_system().write().unwrap();
    let fs = fs_lock.raw();

    let test_cases = [
        ("Japanese Kanji + Kana", "青のすみか - Where Our Blue Is"),
        ("Japanese Katakana", "キタニタツヤ"),
        ("Korean Hangul", "방탄소년단 - 봄날 (Spring Day)"),
        ("Chinese Simplified", "周杰伦 - 晴天"),
        ("Chinese Traditional", "陳奕迅 - 富士山下"),
        ("Arabic", "عمرو دياب - تملي معاك"),
        ("Thai", "ยังคงคอย - Hers"),
        ("Hindi Devanagari", "तुम ही हो - Arijit Singh"),
        ("Cyrillic Russian", "Группа крови - Кино"),
        ("Greek", "Μες στον παράδεισο"),
    ];

    let attrs = iced::advanced::graphics::text::cosmic_text::Attrs::new()
        .weight(iced::advanced::graphics::text::cosmic_text::Weight::BOLD);

    for (label, sample) in test_cases {
        let mut buffer = iced::advanced::graphics::text::cosmic_text::Buffer::new(fs, iced::advanced::graphics::text::cosmic_text::Metrics::new(16.0, 20.0));
        buffer.set_text(fs, sample, attrs, iced::advanced::graphics::text::cosmic_text::Shaping::Advanced);
        buffer.shape_until_scroll(fs, false);

        let mut missing_glyphs = 0;
        let mut total_glyphs = 0;
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                total_glyphs += 1;
                if glyph.glyph_id == 0 {
                    missing_glyphs += 1;
                }
            }
        }
        println!("[{}] total={}, missing={}", label, total_glyphs, missing_glyphs);
        assert_eq!(missing_glyphs, 0, "Language '{}' has missing glyphs!", label);
    }
}
