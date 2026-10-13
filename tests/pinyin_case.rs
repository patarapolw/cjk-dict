use yomitan_parser::pinyin::{Opts, Toneless, Umlaut, normalize_pinyin}; // your crate's lib name

const COLON: Opts = Opts {
    toneless: Toneless::Neutral,
    umlaut: Umlaut::Colon,
};

/// Columns: mode,input,expected,note   (mode: dict | query | colon; expected NONE = not pinyin)
/// The note column is free text for humans and is ignored.
#[test]
fn regression_cases_from_csv() {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::All)
        .from_reader(include_str!("data/pinyin_cases.csv").as_bytes());

    let mut count = 0;
    for rec in rdr.records() {
        let rec = rec.expect("malformed CSV row");
        let line = rec.position().map_or(0, |p| p.line());
        let (mode, input, expected) = (&rec[0], &rec[1], &rec[2]);

        let opts = match mode {
            "dict" => Opts::DICT,
            "query" => Opts::QUERY,
            "colon" => COLON,
            other => panic!("pinyin_cases.csv line {line}: unknown mode {other:?}"),
        };
        let expected = (expected != "NONE").then_some(expected);
        assert_eq!(
            normalize_pinyin(input, opts).as_deref(),
            expected,
            "pinyin_cases.csv line {line}: {mode} {input:?}"
        );
        count += 1;
    }
    assert!(count > 0, "no test cases were read");
}
