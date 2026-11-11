//! SCORM 1.2 Run-Time Environment API trait.

/// Interface to the SCORM 1.2 `window.API` object (or a test fake).
///
/// Methods correspond to standard SCORM 1.2 API functions:
/// `LMSInitialize`, `LMSGetValue`, `LMSSetValue`, `LMSCommit`,
/// `LMSFinish`, and `LMSGetLastError` / `LMSGetErrorString`.
pub trait CmiApi {
    /// Initialize communication with the LMS (`LMSInitialize("")`).
    fn initialize(&mut self) -> bool;

    /// Read a data model element (`LMSGetValue(key)`).
    fn get_value(&mut self, key: &str) -> String;

    /// Write a data model element (`LMSSetValue(key, value)`).
    fn set_value(&mut self, key: &str, value: &str) -> bool;

    /// Persist cached data to the LMS (`LMSCommit("")`).
    fn commit(&mut self) -> bool;

    /// End communication with the LMS (`LMSFinish("")`).
    fn finish(&mut self) -> bool;

    /// Return the last error information (`LMSGetLastError` + `LMSGetErrorString`).
    fn last_error(&mut self) -> String;
}

impl<T: CmiApi + ?Sized> CmiApi for &mut T {
    fn initialize(&mut self) -> bool {
        (**self).initialize()
    }

    fn get_value(&mut self, key: &str) -> String {
        (**self).get_value(key)
    }

    fn set_value(&mut self, key: &str, value: &str) -> bool {
        (**self).set_value(key, value)
    }

    fn commit(&mut self) -> bool {
        (**self).commit()
    }

    fn finish(&mut self) -> bool {
        (**self).finish()
    }

    fn last_error(&mut self) -> String {
        (**self).last_error()
    }
}
