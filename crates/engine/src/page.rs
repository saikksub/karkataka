//! What the engine puts into the page and reads back out of it: the data URL it loads, and the
//! hidden marker row that tells which frame a paint shows.
//!
//! Chromium can paint an older frame, sometimes several begin frames late. So each paint says
//! which frame it shows: the view is one row taller than the video, and that hidden row holds
//! black/white marker cells with the frame number, set in the same JavaScript task as
//! `__setFrame(n)`. A paint is accepted only when its marker reads `n`; the row is cropped off.

/// Marker cells in the extra bottom row: 16 bits of frame number, then a white and a black
/// cell that confirm the row was drawn at all.
const MARKER_CELL: usize = 4;
const MARKER_BITS: usize = 16;

/// Creates the marker row and `__karkaFrame(n)`, which applies frame `n` (waiting for it when
/// `__setFrame` returns a promise) and then sets the marker. `{H}` is the video height.
const MARKER_SCRIPT: &str = r##"(() => {
  const row = document.createElement("div");
  row.style.cssText = "position:fixed;left:0;top:{H}px;height:1px;display:flex;z-index:2147483647;pointer-events:none";
  const cells = [];
  for (let i = 0; i < 18; i++) {
    const c = document.createElement("div");
    c.style.cssText = "width:4px;height:1px;background:" + (i === 17 ? "#000" : "#fff");
    row.appendChild(c);
    cells.push(c);
  }
  document.documentElement.appendChild(row);
  window.__karkaFrame = (n) =>
    Promise.resolve(window.__setFrame(n)).then(() => {
      for (let i = 0; i < 16; i++) cells[i].style.background = (n >> i) & 1 ? "#fff" : "#000";
    });
})();"##;

/// The script to run once the page has loaded, for a video `height` px tall.
pub(crate) fn marker_script(height: u32) -> String {
    MARKER_SCRIPT.replace("{H}", &height.to_string())
}

/// Console messages starting with this carry the page's report (see `report_script`).
pub(crate) const REPORT_PREFIX: &str = "__karka_report:";

/// Run once after load, before frame 0: logs `window.__karkaReport()` (a string the page builds,
/// such as its audio clips) for the engine to pick up. Pages without it log nothing.
pub(crate) const REPORT_SCRIPT: &str = r#"if (window.__karkaReport) {
  let report;
  try { report = String(window.__karkaReport()); } catch (e) { report = JSON.stringify({ error: String(e) }); }
  console.log("__karka_report:" + report);
}"#;

/// The script that starts frame `n`.
pub(crate) fn frame_script(n: u32) -> String {
    format!("window.__karkaFrame({n})")
}

/// Frame number in the marker row of a `width` x (`height` + 1) BGRA paint, if the row is drawn.
pub(crate) fn read_marker(pixels: &[u8], width: usize, height: usize) -> Option<u32> {
    let row = &pixels[height * width * 4..(height + 1) * width * 4];
    let cell = |i: usize| -> Option<bool> {
        let at = (i * MARKER_CELL + MARKER_CELL / 2) * 4;
        match row[at..at + 4] {
            [b, g, r, 255] if b < 16 && g < 16 && r < 16 => Some(false),
            [b, g, r, 255] if b > 239 && g > 239 && r > 239 => Some(true),
            _ => None,
        }
    };
    if !cell(MARKER_BITS)? || cell(MARKER_BITS + 1)? {
        return None;
    }
    (0..MARKER_BITS).try_fold(0, |n, i| Some(n | (u32::from(cell(i)?) << i)))
}

pub(crate) fn data_url(html: &str) -> String {
    let mut url = String::from("data:text/html;charset=utf-8,");
    for b in html.bytes() {
        if b.is_ascii_alphanumeric() || b".-_~".contains(&b) {
            url.push(b as char);
        } else {
            url.push_str(&format!("%{b:02X}"));
        }
    }
    url
}

#[test]
fn data_url_escapes_markup() {
    assert_eq!(data_url("<b>a b#</b>"), "data:text/html;charset=utf-8,%3Cb%3Ea%20b%23%3C%2Fb%3E");
}

#[test]
fn reads_marker_row() {
    let (w, h) = (80, 2);
    let mut px = vec![0u8; w * (h + 1) * 4];
    let set = |px: &mut Vec<u8>, i: usize, white: bool| {
        let v = if white { 255 } else { 0 };
        for x in i * MARKER_CELL..(i + 1) * MARKER_CELL {
            let at = (h * w + x) * 4;
            px[at..at + 4].copy_from_slice(&[v, v, v, 255]);
        }
    };
    // Not drawn yet (transparent): no frame.
    assert_eq!(read_marker(&px, w, h), None);
    for i in 0..MARKER_BITS {
        set(&mut px, i, [1, 4].contains(&i));
    }
    // Sentinels swapped: the row is not trusted.
    set(&mut px, MARKER_BITS, false);
    set(&mut px, MARKER_BITS + 1, true);
    assert_eq!(read_marker(&px, w, h), None);
    set(&mut px, MARKER_BITS, true);
    set(&mut px, MARKER_BITS + 1, false);
    assert_eq!(read_marker(&px, w, h), Some(0b10010));
}

#[test]
fn half_drawn_marker_rows_are_not_trusted() {
    let (w, h) = (80, 2);
    let paint = |cells: &[(usize, [u8; 4])]| {
        let mut px = vec![0u8; w * (h + 1) * 4];
        for &(i, color) in cells {
            for x in i * MARKER_CELL..(i + 1) * MARKER_CELL {
                px[(h * w + x) * 4..][..4].copy_from_slice(&color);
            }
        }
        px
    };
    let (white, black) = ([255; 4], [0, 0, 0, 255]);
    // Only the first check cell is drawn.
    assert_eq!(read_marker(&paint(&[(MARKER_BITS, white)]), w, h), None);
    // Both check cells are drawn, but the bit cells are not.
    assert_eq!(read_marker(&paint(&[(MARKER_BITS, white), (MARKER_BITS + 1, black)]), w, h), None);
}

#[test]
fn scripts_carry_height_and_frame() {
    assert!(marker_script(1920).contains("top:1920px"));
    assert_eq!(frame_script(42), "window.__karkaFrame(42)");
}
