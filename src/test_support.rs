//! Report-boundary recorder: no device handles or desktop input.

use std::cell::RefCell;

use crate::device::ReportSink;
use crate::{Error, Result};

#[derive(Default)]
pub(crate) struct Reports {
    attempts: RefCell<Vec<(u32, [u8; 8])>>,
    accepted: RefCell<Vec<(u32, [u8; 8])>>,
    failures: RefCell<Vec<usize>>,
    rejected_reports: RefCell<Vec<(u32, [u8; 8])>>,
}

impl Reports {
    pub(crate) fn fail_on(&self, attempts: &[usize]) {
        self.failures.borrow_mut().extend_from_slice(attempts);
    }

    pub(crate) fn reject_report(&self, code: u32, report: [u8; 8]) {
        self.rejected_reports.borrow_mut().push((code, report));
    }

    pub(crate) fn count(&self) -> usize {
        self.attempts.borrow().len()
    }

    pub(crate) fn accepted(&self) -> Vec<(u32, [u8; 8])> {
        self.accepted.borrow().clone()
    }

    pub(crate) fn last(&self) -> [u8; 8] {
        self.accepted.borrow().last().expect("accepted report").1
    }
}

impl ReportSink for Reports {
    fn send_report(&self, code: u32, report: &[u8; 8]) -> Result<()> {
        self.attempts.borrow_mut().push((code, *report));
        let attempt = self.count();
        if self.failures.borrow().contains(&attempt)
            || self.rejected_reports.borrow().contains(&(code, *report))
        {
            return Err(Error::io(
                "test report transport",
                std::io::Error::from_raw_os_error(attempt as i32),
            ));
        }
        self.accepted.borrow_mut().push((code, *report));
        Ok(())
    }
}
