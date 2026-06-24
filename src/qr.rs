//! Rendering of strings (addresses) as terminal QR codes.

use qrcode::{EcLevel, QrCode};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

/// A rendered QR code, with the terminal dimensions it occupies.
pub(crate) struct RenderedQr {
    pub(crate) lines: Vec<Line<'static>>,
    /// Width in terminal columns.
    pub(crate) width: u16,
    /// Height in terminal rows.
    pub(crate) height: u16,
}

/// Renders the given data as a QR code, returning one [`Line`] per two rows of QR modules.
///
/// Two vertical modules are packed into each character cell using the unicode half-block
/// characters, so the resulting QR code is roughly square in a typical terminal. Returns
/// `None` if the data cannot be encoded as a QR code (e.g. it is too long).
pub(crate) fn render(data: &str) -> Option<RenderedQr> {
    let code = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::M).ok()?;
    let module_width = code.width();
    let modules = code.to_colors();

    // Horizontal quiet zone, in modules, on the left and right. The QR spec recommends 4;
    // we use a smaller zone to conserve terminal space, which scanners tolerate in practice.
    const QUIET_X: usize = 2;

    // Two module rows are packed into each character cell using half-block glyphs, so each
    // text line spans two grid rows. We use a single blank text line as the top quiet zone
    // (2 module rows) and align the module area to start on the next text line.
    //
    // The bottom quiet zone is handled implicitly: the grid height is chosen so the final
    // text line holds the last module row in its top half and a light (white) bottom half.
    // That white half-line is the bottom quiet zone. We deliberately do NOT emit an extra
    // fully-blank bottom line — doing so previously produced a stray white bar floating
    // below the code.
    const QUIET_TOP: usize = 2;
    let grid_width = module_width + QUIET_X * 2;
    // Total grid rows: top quiet (2) + module rows, rounded up to an even number so the
    // final text line is half module / half light bottom quiet zone. module_width is always
    // odd, and QUIET_TOP is even, so the sum is odd and we add one light row.
    let body_plus_top = QUIET_TOP + module_width;
    let grid_height = body_plus_top + (body_plus_top % 2);
    debug_assert!(grid_height % 2 == 0);

    // `dark(x, y)` is true when the module at grid coordinate `(x, y)` is dark. The quiet
    // zone is light.
    let dark = |x: usize, y: usize| -> bool {
        if x < QUIET_X || y < QUIET_TOP {
            return false;
        }
        let (mx, my) = (x - QUIET_X, y - QUIET_TOP);
        if mx >= module_width || my >= module_width {
            return false;
        }
        modules[my * module_width + mx] == qrcode::Color::Dark
    };

    // Explicitly pin dark modules to black and light modules to white, rather than relying
    // on the terminal's default colors. The half-block glyphs encode the top module in the
    // cell's foreground and the bottom module in its background, so with `fg = black` and
    // `bg = white` every module renders dark-on-light regardless of the terminal's theme
    // (a light-on-dark terminal would otherwise produce an inverted code that some scanners
    // reject).
    let style = Style::default().fg(Color::Black).bg(Color::White);

    let mut lines = Vec::with_capacity(grid_height / 2);
    let mut y = 0;
    while y < grid_height {
        let mut s = String::with_capacity(grid_width);
        for x in 0..grid_width {
            let top = dark(x, y);
            let bottom = dark(x, y + 1);
            // Foreground = dark module. Use half blocks so two rows fit one cell.
            s.push(match (top, bottom) {
                (true, true) => '\u{2588}',  // full block
                (true, false) => '\u{2580}', // upper half block
                (false, true) => '\u{2584}', // lower half block
                (false, false) => ' ',
            });
        }
        lines.push(Line::from(Span::styled(s, style)));
        y += 2;
    }

    let height = lines.len() as u16;
    Some(RenderedQr {
        lines,
        width: grid_width as u16,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rendered grid must be square-ish and have no ragged final row: every line is the
    /// same width, and the height accounts for all module rows plus symmetric quiet zones.
    #[test]
    fn dimensions_are_consistent() {
        let rendered = render("u1exampleaddressdata1234567890").expect("encodable");
        // All lines have equal display width.
        let w = rendered.width as usize;
        for line in &rendered.lines {
            let line_w: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
            assert_eq!(line_w, w, "every QR line should be the full width");
        }
        assert_eq!(rendered.height as usize, rendered.lines.len());
        // Width and height should be within one cell of each other (square QR, 2 rows/cell).
        let h_modules = rendered.height as i32 * 2;
        assert!((h_modules - rendered.width as i32).abs() <= 2);
    }

    #[test]
    fn final_line_is_not_a_floating_blank_bar() {
        // Regression: the bottom quiet zone was previously emitted as a separate, fully
        // blank text line, which floated below the code as a stray white bar. The bottom
        // quiet zone is now carried by the white bottom half of the final module row's
        // line, so the last rendered line must NOT be fully blank.
        for data in [
            "u1exampleaddressdata1234567890",
            "utest1rg2f7p4vujffeyutx602meuwlzeen25djn9gdztlxeg59luvu9epj7yqx0",
            "t1abcdefghijklmnopqrstuvwxyz0123456789",
        ] {
            let rendered = render(data).expect("encodable");
            let is_blank = |l: &Line| l.spans.iter().all(|s| s.content.chars().all(|c| c == ' '));
            let last = rendered.lines.last().expect("at least one line");
            assert!(
                !is_blank(last),
                "the final QR line should carry the last module row, not be a blank bar (data: {data})"
            );
        }
    }
}
