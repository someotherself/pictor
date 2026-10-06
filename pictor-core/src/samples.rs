/// Sample data that either borrows an existing slice or owns a `Vec`.
///
/// Borrowed storage avoids a copy when data can be reused as-is; owned storage
/// holds data produced by a conversion. The stored elements may be sample values
/// or bytes representing samples, depending on the codec.
pub enum SampleStorage<'a, S: Sample> {
    Borrow { data: &'a [S] },
    Owned { data: Vec<S> },
}

impl<'a, S: Sample> SampleStorage<'a, S> {
    pub fn get_data(&self) -> &[S] {
        match self {
            Self::Owned { data } => data,
            Self::Borrow { data } => data,
        }
    }

    /// Retrive the length of the underlying storage. NOT the number of samples / pixels
    pub fn len(&self) -> usize {
        match self {
            Self::Owned { data } => data.len(),
            Self::Borrow { data } => data.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Owned { data } => data.is_empty(),
            Self::Borrow { data } => data.is_empty(),
        }
    }

    pub fn is_owned(&self) -> bool {
        matches!(self, SampleStorage::Owned { data: _ })
    }

    pub fn is_borrowed(&self) -> bool {
        matches!(self, SampleStorage::Borrow { data: _ })
    }

    /// Avoids consuming self, and returning `SampleStorage::Borrow` instead
    pub fn as_borrowed(&self) -> SampleStorage<'_, S> {
        SampleStorage::Borrow {
            data: self.get_data(),
        }
    }

    /// Consumes and returns an owned vec.
    ///
    /// Will allocate if data is borrowed
    pub fn into_vec(self) -> Vec<S> {
        match self {
            Self::Owned { data } => data,
            Self::Borrow { data } => data.to_vec(),
        }
    }

    /// Try and consume returning an owned Vec without allocating.
    ///
    /// Will return `None` if data is borrowed
    pub fn try_into_vec(self) -> Option<Vec<S>> {
        match self {
            Self::Owned { data } => Some(data),
            Self::Borrow { data: _ } => None,
        }
    }
}

mod sealed {
    pub trait Sealed {}

    impl Sealed for u8 {}
    impl Sealed for u16 {}
}

/// Helper trait samples types supported by the image codecs and conversions
pub trait Sample: Copy + Sync + sealed::Sealed {
    const BYTES_PER_SAMPLE: usize;
    const BIT_DEPTH: u8;

    /// Used by image formats that expect be bytes, such as `PNG`
    fn into_be_bytes(buffer: SampleStorage<'_, Self>) -> SampleStorage<'_, u8>;
    /// Used for image formats that use be bytes, such as `PNG`
    fn from_be_bytes(bytes: &[u8]) -> SampleStorage<'_, Self>;

    fn downsample_to_u8_samples<'a>(input: SampleStorage<'a, Self>) -> SampleStorage<'a, u8>;

    /// Helper for `downsample_to_u8_samples`
    #[inline]
    fn downsample_u16_to_u8(sample: u16) -> u8 {
        ((sample as u32 * 255 + 32767) / 65535) as u8
    }

    fn upsample_to_u16_samples<'a>(input: SampleStorage<'a, Self>) -> SampleStorage<'a, u16>;

    /// Helper for `upsample_to_u16_samples`
    #[inline]
    fn upsample_u8_to_u16(sample: u8) -> u16 {
        u16::from(sample) * 257
    }
}

impl Sample for u8 {
    const BYTES_PER_SAMPLE: usize = 1;
    const BIT_DEPTH: u8 = 8;

    fn into_be_bytes<'a>(buffer: SampleStorage<'a, Self>) -> SampleStorage<'a, u8> {
        buffer
    }

    fn from_be_bytes<'a>(bytes: &'a [u8]) -> SampleStorage<'a, Self> {
        SampleStorage::Borrow { data: bytes }
    }

    fn downsample_to_u8_samples<'a>(input: SampleStorage<'a, Self>) -> SampleStorage<'a, u8> {
        input
    }

    fn upsample_to_u16_samples<'a>(input: SampleStorage<'a, Self>) -> SampleStorage<'a, u16> {
        let data = input
            .get_data()
            .iter()
            .copied()
            .map(Self::upsample_u8_to_u16)
            .collect();

        SampleStorage::Owned { data }
    }
}

impl Sample for u16 {
    const BYTES_PER_SAMPLE: usize = 2;
    const BIT_DEPTH: u8 = 16;

    fn into_be_bytes<'a>(buffer: SampleStorage<'a, Self>) -> SampleStorage<'a, u8> {
        let out_len = buffer.len() * Self::BYTES_PER_SAMPLE;
        let mut out = Vec::with_capacity(out_len);
        for item in buffer.get_data() {
            out.extend_from_slice(&item.to_be_bytes());
        }
        SampleStorage::Owned { data: out }
    }

    fn from_be_bytes(bytes: &[u8]) -> SampleStorage<'_, Self> {
        assert_eq!(
            bytes.len() % 2,
            0,
            "u16 samples require an even number of bytes"
        );

        let data = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();

        SampleStorage::Owned { data }
    }

    fn downsample_to_u8_samples<'a>(input: SampleStorage<'a, Self>) -> SampleStorage<'a, u8> {
        let data = match input {
            SampleStorage::Borrow { data } => data
                .iter()
                .map(|&sample| Self::downsample_u16_to_u8(sample))
                .collect(),

            SampleStorage::Owned { data } => {
                data.into_iter().map(Self::downsample_u16_to_u8).collect()
            }
        };

        SampleStorage::Owned { data }
    }

    fn upsample_to_u16_samples<'a>(input: SampleStorage<'a, Self>) -> SampleStorage<'a, u16> {
        input
    }
}

pub enum SampleFormat {
    U8,
    U16,
}
