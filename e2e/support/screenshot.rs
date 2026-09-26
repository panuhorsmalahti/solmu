pub fn capture_terminal(screen: &vt100::Screen, name: &str) {
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_none() {
        return;
    }
    fn color(value: vt100::Color) -> String {
        match value {
            vt100::Color::Default => "inherit".into(),
            vt100::Color::Rgb(r, g, b) => format!("rgb({r},{g},{b})"),
            vt100::Color::Idx(index) => {
                const PALETTE: [&str; 16] = [
                    "#151a23", "#eb6f92", "#9ccfd8", "#f6c177", "#719cd6", "#c4a7e7", "#63cdcf",
                    "#e0def4", "#747986", "#eb6f92", "#9ccfd8", "#f6c177", "#91b4df", "#c4a7e7",
                    "#9be7e8", "#ffffff",
                ];
                if index < 16 {
                    PALETTE[index as usize].into()
                } else if index < 232 {
                    let value = index - 16;
                    let component = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                    format!(
                        "rgb({},{},{})",
                        component(value / 36),
                        component(value / 6 % 6),
                        component(value % 6)
                    )
                } else {
                    let value = 8 + (index - 232) * 10;
                    format!("rgb({value},{value},{value})")
                }
            }
        }
    }

    let mut html = String::from(
        "<!doctype html><meta charset=utf-8><style>body{margin:0;background:#151a23;color:#e0def4;padding:24px}pre{margin:0;font:14px/20px Consolas,'DejaVu Sans Mono',monospace}span{display:inline-block;width:1ch;height:20px}</style><pre>",
    );
    let (rows, columns) = screen.size();
    for row in 0..rows {
        for column in 0..columns {
            let cell = screen.cell(row, column).unwrap();
            if cell.is_wide_continuation() {
                continue;
            }
            let contents = cell.contents();
            let contents = if contents.is_empty() {
                " ".into()
            } else {
                contents
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
            };
            html.push_str(&format!("<span style=\"color:{};background:{};font-weight:{};width:{}ch\">{contents}</span>", color(cell.fgcolor()), color(cell.bgcolor()), if cell.bold() { "700" } else { "400" }, if cell.is_wide() { 2 } else { 1 }));
        }
        html.push('\n');
    }
    html.push_str("</pre>");
    let directory = super::root().join("artifacts");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join(format!("{name}.html")), html).unwrap();
}
