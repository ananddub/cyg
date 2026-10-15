//! The forward-only sequence mark, including the wrap.

use super::SeqWatermark;

#[test]
fn stale_seq_is_counted_and_never_moves_the_mark_backwards() {
    let mut mark = SeqWatermark::default();
    assert_eq!(mark.get(), None);
    assert!(mark.advance(0), "the first seq is always fresh");
    assert_eq!(mark.get(), Some(0));

    assert!(mark.advance(1));
    assert!(mark.advance(2));
    assert_eq!(mark.stale(), 0);

    // Duplicate and late are both stale.
    assert!(!mark.advance(2));
    assert!(!mark.advance(1));
    assert!(mark.is_stale(1));
    assert_eq!(mark.stale(), 2);
    assert_eq!(mark.get(), Some(2), "the mark only moves forward");

    // Wrap-safe, exactly like the host's newest-wins rule: from 2, 65535 is *behind* and 0 is ahead.
    assert!(!mark.advance(u16::MAX), "65535 is behind 2 in the wrap window");
    assert!(mark.advance(3));
}

#[test]
fn the_watermark_wraps_forward_once_and_rejects_the_old_side() {
    let mut mark = SeqWatermark::default();
    assert!(mark.advance(65534));
    assert!(mark.advance(65535));
    assert!(!mark.advance(65535), "duplicate");
    assert!(mark.advance(0), "the counter wraps forward");
    assert_eq!(mark.get(), Some(0));
    assert!(!mark.advance(65535), "the pre-wrap seq is behind again");
    assert!(!mark.advance(0), "duplicate after the wrap");
    assert_eq!(mark.stale(), 3);
}
