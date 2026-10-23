//! UB tests for `libavutil/bprint.h`'s unsafe
//! [`BPrintFieldsMut::set_str_borrowed`], each call within its `# Safety`
//! contract: the string stays valid and unwritten until no copy of the value
//! can return it.
//!
//! [`BPrintFieldsMut::set_str_borrowed`]: ffmpeg_bindings::libavutil::libavutil::bprint::BPrintFieldsMut::set_str_borrowed

use core::ffi::CStr;
use std::ffi::CString;
use std::thread;

use ffmpeg_bindings::libavutil::libavutil::bprint::{BPrintFields, BPrintFieldsMut};

/// Stores `s`, within the contract: per each caller, `s` outlives every read
/// of it through the value and its copies.
fn store(mut m: BPrintFieldsMut<'_>, s: Option<&CStr>) {
    // SAFETY: per each caller, `s` stays alive and unwritten until the field
    // is overwritten or the value and every copy of it are dropped.
    unsafe { m.set_str_borrowed(s) };
}

#[test]
fn a_heap_string_is_read_while_it_lives() {
    for text in ["", "x", "so far", &"long ".repeat(200)] {
        let s = CString::new(text).unwrap();
        let mut f = BPrintFields::new();
        store(f.as_mut(), Some(&s));
        assert_eq!(f.as_ref().str(), Some(s.as_c_str()));
        assert_eq!(f.as_ref().str().unwrap().as_ptr(), s.as_ptr());
        // The value's last read precedes the string's drop.
        drop(s);
    }
}

#[test]
fn none_stores_null() {
    let s = CString::new("gone").unwrap();
    let mut f = BPrintFields::new();
    store(f.as_mut(), Some(&s));
    store(f.as_mut(), None);
    // Nothing refers to `s` any more: it may go first.
    drop(s);
    assert_eq!(f.as_ref().str(), None);
}

#[test]
fn an_overwritten_string_may_be_freed_before_the_value() {
    let first = CString::new("first").unwrap();
    let second = CString::new("second").unwrap();
    let mut f = BPrintFields::new();
    store(f.as_mut(), Some(&first));
    f.as_mut().set_len(5);
    store(f.as_mut(), Some(&second));
    drop(first);
    assert_eq!(f.as_ref().str(), Some(c"second"));
    assert_eq!(f.as_ref().len(), 5);
    // A `'static` string replaces the borrowed one through the safe setter.
    f.as_mut().set_str(Some(c"static"));
    drop(second);
    assert_eq!(f.as_ref().str(), Some(c"static"));
}

#[test]
fn copies_keep_the_string_until_each_is_overwritten() {
    let s = CString::new("shared").unwrap();
    let mut a = BPrintFields::new();
    store(a.as_mut(), Some(&s));
    let mut b = a;
    let c = a;
    assert_eq!(b.as_ref().str().unwrap().as_ptr(), s.as_ptr());
    store(b.as_mut(), None);
    store(a.as_mut(), Some(c"other"));
    // `c` still reads `s`, which lives until the end of the scope.
    assert_eq!(c.as_ref().str(), Some(s.as_c_str()));
    assert_eq!((a.as_ref().str(), b.as_ref().str()), (Some(c"other"), None));
    drop(s);
}

#[test]
fn a_borrowed_string_is_read_from_many_threads() {
    let s = CString::new("threads").unwrap();
    let mut f = BPrintFields::new();
    store(f.as_mut(), Some(&s));
    let f = f;
    // Every thread joins before `s` drops, and nothing writes `s`.
    thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                let mut copy = f;
                for i in 0..500 {
                    assert_eq!(f.as_ref().str(), Some(s.as_c_str()));
                    copy.as_mut().set_len(i);
                }
            });
        }
    });
    drop(s);
}
