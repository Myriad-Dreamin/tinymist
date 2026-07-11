//! The computation for pdf export.

use chrono::{Datelike, Offset, Timelike};
use tinymist_world::args::PdfStandard;
use typst::foundations::Datetime;
pub use typst_pdf::PdfStandard as TypstPdfStandard;
pub use typst_pdf::pdf;

use typst_pdf::{PdfOptions, PdfStandards, Timestamp};

use super::*;
use crate::model::ExportPdfTask;

/// The computation for pdf export.
pub struct PdfExport;

impl<F: CompilerFeat> ExportComputation<F, TypstPagedDocument> for PdfExport {
    type Output = Bytes;
    type Config = ExportPdfTask;

    fn run(
        _graph: &Arc<WorldComputeGraph<F>>,
        doc: &Arc<TypstPagedDocument>,
        config: &ExportPdfTask,
    ) -> Result<Bytes> {
        let options = pdf_options(
            config.pages.as_deref(),
            &config.pdf_standards,
            config.no_pdf_tags,
            config.creation_timestamp,
        )?;

        // log::info!("used options for pdf export: {options:?}");

        // todo: Some(pdf_uri.as_str())
        // todo: ident option
        Ok(Bytes::new(typst_pdf::pdf(doc, &options)?))
    }
}

/// Creates PDF options from shared project export arguments.
pub fn pdf_options(
    pages: Option<&[Pages]>,
    pdf_standards: &[PdfStandard],
    no_pdf_tags: bool,
    creation_timestamp: Option<i64>,
) -> Result<PdfOptions> {
    // Match Typst CLI semantics: explicit timestamps stay in UTC for
    // reproducible builds, while the default records local wall time and its
    // UTC offset.
    let timestamp = match creation_timestamp {
        Some(timestamp) => {
            let datetime = chrono::DateTime::from_timestamp(timestamp, 0)
                .context("timestamp is out of range")?;
            convert_datetime(datetime).map(Timestamp::new_utc)
        }
        None => local_timestamp(chrono::Local::now()),
    };

    let standards = PdfStandards::new(
        &pdf_standards
            .iter()
            .map(|standard| match standard {
                PdfStandard::V_1_4 => typst_pdf::PdfStandard::V_1_4,
                PdfStandard::V_1_5 => typst_pdf::PdfStandard::V_1_5,
                PdfStandard::V_1_6 => typst_pdf::PdfStandard::V_1_6,
                PdfStandard::V_1_7 => typst_pdf::PdfStandard::V_1_7,
                PdfStandard::V_2_0 => typst_pdf::PdfStandard::V_2_0,
                PdfStandard::A_1b => typst_pdf::PdfStandard::A_1b,
                PdfStandard::A_1a => typst_pdf::PdfStandard::A_1a,
                PdfStandard::A_2b => typst_pdf::PdfStandard::A_2b,
                PdfStandard::A_2u => typst_pdf::PdfStandard::A_2u,
                PdfStandard::A_2a => typst_pdf::PdfStandard::A_2a,
                PdfStandard::A_3b => typst_pdf::PdfStandard::A_3b,
                PdfStandard::A_3u => typst_pdf::PdfStandard::A_3u,
                PdfStandard::A_3a => typst_pdf::PdfStandard::A_3a,
                PdfStandard::A_4 => typst_pdf::PdfStandard::A_4,
                PdfStandard::A_4f => typst_pdf::PdfStandard::A_4f,
                PdfStandard::A_4e => typst_pdf::PdfStandard::A_4e,
                PdfStandard::Ua_1 => typst_pdf::PdfStandard::Ua_1,
            })
            .collect::<Vec<_>>(),
    )
    .map_err(|err| err.message().clone())
    .context("prepare pdf standards")?;

    let tagged = !no_pdf_tags && pages.is_none();
    // todo: emit warning diag
    if pages.is_some() && !no_pdf_tags {
        log::warn!(
            "the resulting PDF will be inaccessible because using --pages implies --no-pdf-tags"
        );
    }
    if !tagged {
        const ACCESSIBLE: &[(PdfStandard, &str)] = &[
            (PdfStandard::A_1a, "PDF/A-1a"),
            (PdfStandard::A_2a, "PDF/A-2a"),
            (PdfStandard::A_3a, "PDF/A-3a"),
            (PdfStandard::Ua_1, "PDF/UA-1"),
        ];

        for (standard, name) in ACCESSIBLE {
            if pdf_standards.contains(standard) {
                if no_pdf_tags {
                    log::warn!("cannot disable PDF tags when exporting a {name} document");
                } else {
                    log::warn!(
                        "cannot disable PDF tags when exporting a {name} document. Hint: using --pages implies --no-pdf-tags"
                    );
                }
            }
        }
    }

    Ok(PdfOptions {
        page_ranges: pages.map(exported_page_ranges),
        timestamp,
        standards,
        tagged,
        ..Default::default()
    })
}

fn local_timestamp<Tz: chrono::TimeZone>(
    local_datetime: chrono::DateTime<Tz>,
) -> Option<Timestamp> {
    let minute_offset = local_datetime.offset().fix().local_minus_utc() / 60;
    convert_datetime(local_datetime)
        .and_then(|datetime| Timestamp::new_local(datetime, minute_offset))
}

fn convert_datetime<Tz: chrono::TimeZone>(date_time: chrono::DateTime<Tz>) -> Option<Datetime> {
    Datetime::from_ymd_hms(
        date_time.year(),
        date_time.month().try_into().ok()?,
        date_time.day().try_into().ok()?,
        date_time.hour().try_into().ok()?,
        date_time.minute().try_into().ok()?,
        date_time.second().try_into().ok()?,
    )
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn export_with_timestamp(timestamp: Timestamp) -> Vec<u8> {
        let document = TypstPagedDocument::new(Default::default(), Default::default());
        typst_pdf::pdf(
            &document,
            &PdfOptions {
                timestamp: Some(timestamp),
                tagged: false,
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn assert_pdf_contains(pdf: &[u8], expected: &str) {
        assert!(
            pdf.windows(expected.len())
                .any(|window| window == expected.as_bytes()),
            "PDF metadata should contain {expected}"
        );
    }

    fn assert_pdf_dates(pdf: &[u8], pdf_date: &str, xmp_date: &str) {
        for field in ["CreationDate", "ModDate"] {
            assert_pdf_contains(pdf, &format!("/{field}({pdf_date})"));
        }
        for field in ["CreateDate", "ModifyDate"] {
            assert_pdf_contains(pdf, &format!("<xmp:{field}>{xmp_date}</xmp:{field}>"));
        }
    }

    #[test]
    fn default_pdf_timestamp_uses_local_timezone() {
        let options = pdf_options(None, &[], false, None).unwrap();
        let timestamp = options.timestamp.unwrap();

        assert!(
            format!("{timestamp:?}").contains("timezone: Local"),
            "default PDF timestamp should retain the local timezone: {timestamp:?}"
        );
    }

    #[test]
    fn explicit_pdf_timestamp_uses_utc() {
        let options = pdf_options(None, &[], false, Some(0)).unwrap();
        let timestamp = options.timestamp.unwrap();
        let pdf = export_with_timestamp(timestamp);

        assert_pdf_dates(&pdf, "D:19700101000000Z", "1970-01-01T00:00:00+00:00");
    }

    #[test]
    fn explicit_timestamp_outside_typst_range_is_omitted() {
        // Chrono accepts this instant, but Typst's datetime ends at year 9999.
        let options = pdf_options(None, &[], false, Some(253_402_300_800)).unwrap();

        assert!(options.timestamp.is_none());
    }

    #[test]
    fn invalid_explicit_timestamp_is_rejected() {
        let error = pdf_options(None, &[], false, Some(i64::MAX)).unwrap_err();

        assert_eq!(error.to_string(), "timestamp is out of range");
    }

    #[test]
    fn local_pdf_timestamp_preserves_wall_time_and_offset() {
        for (seconds, pdf_date, xmp_date) in [
            (
                5 * 60 * 60 + 30 * 60,
                "D:20241217101112+05'30",
                "2024-12-17T10:11:12+05:30",
            ),
            (
                -(3 * 60 * 60 + 30 * 60),
                "D:20241217101112-03'30",
                "2024-12-17T10:11:12-03:30",
            ),
        ] {
            let offset = chrono::FixedOffset::east_opt(seconds).unwrap();
            let local_datetime = offset
                .with_ymd_and_hms(2024, 12, 17, 10, 11, 12)
                .single()
                .unwrap();
            let timestamp = local_timestamp(local_datetime).unwrap();
            let pdf = export_with_timestamp(timestamp);

            assert_pdf_dates(&pdf, pdf_date, xmp_date);
        }
    }
}
