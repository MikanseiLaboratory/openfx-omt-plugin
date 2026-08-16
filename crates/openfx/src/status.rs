use std::ffi::c_int;
use std::fmt;

/// OpenFX status code. Bindgen cannot import `OfxStatus` as a newtype, so we wrap it.
#[must_use]
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct OfxStatus(pub c_int);

pub type OfxResult<T> = Result<T, OfxStatus>;

impl OfxStatus {
    pub const fn is_ok(self) -> bool {
        self.0 == kOfxStat::OK.0
    }

    pub fn ofx_ok(self) -> OfxResult<()> {
        if self.is_ok() { Ok(()) } else { Err(self) }
    }
}

impl fmt::Debug for OfxStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.0 {
            0 => "kOfxStatOK",
            1 => "kOfxStatFailed",
            2 => "kOfxStatErrFatal",
            3 => "kOfxStatErrUnknown",
            4 => "kOfxStatErrMissingHostFeature",
            5 => "kOfxStatErrUnsupported",
            6 => "kOfxStatErrExists",
            7 => "kOfxStatErrFormat",
            8 => "kOfxStatErrMemory",
            9 => "kOfxStatErrBadHandle",
            10 => "kOfxStatErrBadIndex",
            11 => "kOfxStatErrValue",
            12 => "kOfxStatReplyYes",
            13 => "kOfxStatReplyNo",
            14 => "kOfxStatReplyDefault",
            _ => return f.debug_tuple("OfxStatus").field(&self.0).finish(),
        };
        f.write_str(name)
    }
}

impl From<c_int> for OfxStatus {
    fn from(value: c_int) -> Self {
        Self(value)
    }
}

/// Named status constants matching the OpenFX C API.
pub mod kOfxStat {
    use super::OfxStatus;

    pub const OK: OfxStatus = OfxStatus(0);
    pub const Failed: OfxStatus = OfxStatus(1);
    pub const ErrFatal: OfxStatus = OfxStatus(2);
    pub const ErrUnknown: OfxStatus = OfxStatus(3);
    pub const ErrMissingHostFeature: OfxStatus = OfxStatus(4);
    pub const ErrUnsupported: OfxStatus = OfxStatus(5);
    pub const ErrExists: OfxStatus = OfxStatus(6);
    pub const ErrFormat: OfxStatus = OfxStatus(7);
    pub const ErrMemory: OfxStatus = OfxStatus(8);
    pub const ErrBadHandle: OfxStatus = OfxStatus(9);
    pub const ErrBadIndex: OfxStatus = OfxStatus(10);
    pub const ErrValue: OfxStatus = OfxStatus(11);
    pub const ReplyYes: OfxStatus = OfxStatus(12);
    pub const ReplyNo: OfxStatus = OfxStatus(13);
    pub const ReplyDefault: OfxStatus = OfxStatus(14);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_converts() {
        assert!(kOfxStat::OK.ofx_ok().is_ok());
        assert!(kOfxStat::Failed.ofx_ok().is_err());
        assert_eq!(
            format!("{:?}", kOfxStat::ReplyDefault),
            "kOfxStatReplyDefault"
        );
        assert_eq!(format!("{:?}", OfxStatus(99)), "OfxStatus(99)");
    }
}
