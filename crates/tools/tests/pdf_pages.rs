use saqviro_jobs::ErrorKind;
use saqviro_tools::pdf::{
    edit::PageSize,
    failure,
    pages::{parse_pages, qpdf_range, reordered, without},
};

#[test]
fn reads_page_lists() {
    assert_eq!(parse_pages("1", 9).unwrap(), [1]);
    assert_eq!(parse_pages("1, 3-5 ,8", 9).unwrap(), [1, 3, 4, 5, 8]);
    assert_eq!(parse_pages("3-1", 9).unwrap(), [3, 2, 1]);
}

#[test]
fn rejects_pages_that_do_not_exist() {
    let e = parse_pages("12", 10).unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidInput);
    assert_eq!(e.message, "there is no page 12, this PDF has 10 pages");
    assert!(parse_pages("0", 10).is_err());
    assert!(parse_pages("2", 1).unwrap_err().message.ends_with("has 1 page"));
}

#[test]
fn rejects_what_is_not_a_page_list() {
    for text in ["", "a", "1-", "-1", "1-2-3", "1,,2", "1.5", "z", "--help", "1;2"] {
        assert!(parse_pages(text, 9).is_err(), "{text}");
    }
}

#[test]
fn keeps_the_other_pages() {
    assert_eq!(without(5, &[2, 4]), [1, 3, 5]);
    assert_eq!(without(2, &[1, 2]), Vec::<u32>::new());
}

#[test]
fn listed_pages_come_first_and_the_rest_follow() {
    assert_eq!(reordered(5, &[4, 1]).unwrap(), [4, 1, 2, 3, 5]);
    assert_eq!(reordered(2, &[2, 1]).unwrap(), [2, 1]);
    assert!(reordered(5, &[1, 1]).unwrap_err().message.contains("twice"));
}

#[test]
fn writes_qpdf_ranges() {
    assert_eq!(qpdf_range(&[3, 1, 2]), "3,1,2");
}

#[test]
fn explains_qpdf_failures() {
    let password = failure("qpdf: a.pdf: invalid password\n");
    assert_eq!(password.kind, ErrorKind::PermissionDenied);
    assert!(password.message.contains("Remove PDF Password"));
    assert_eq!(
        failure("a.pdf: can't find PDF header").kind,
        ErrorKind::UnsupportedFormat
    );
    assert_eq!(failure("write: No space left on device").kind, ErrorKind::DiskFull);
    let other = failure("one\nthe last line\n");
    assert_eq!(
        (other.kind, other.message.as_str()),
        (ErrorKind::ProcessFailed, "the last line")
    );
}

#[test]
fn a_page_fits_its_image() {
    let (page, (x, y, w, h)) = PageSize::Fit.place(800, 400);
    assert_eq!(page, (600.0, 300.0));
    assert_eq!((x, y, w, h), (0.0, 0.0, 600.0, 300.0));
}

#[test]
fn huge_images_stay_inside_the_largest_page_viewers_accept() {
    let (page, _) = PageSize::Fit.place(30_000, 15_000);
    assert!(page.0 <= 14_400.0 && (page.0 / page.1 - 2.0).abs() < 0.01, "{page:?}");
}

#[test]
fn images_are_centered_on_paper_and_turn_it_when_wide() {
    let (page, (x, y, w, h)) = PageSize::A4.place(100, 100);
    assert_eq!(page.0, 595.28);
    assert_eq!((x, w, h), (0.0, 595.28, 595.28));
    assert!((y - (841.89 - 595.28) / 2.0).abs() < 0.01);

    let (page, _) = PageSize::Letter.place(300, 100);
    assert_eq!(page, (792.0, 612.0));
}
