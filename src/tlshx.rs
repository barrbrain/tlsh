use core::str::FromStr;

use crate::pearson::{b_mapping, fast_b_mapping};
use crate::quantile::get_tertiles;
use crate::util::{l_capturing, swap_byte};
use crate::BUCKETS;

const SLIDING_WND_SIZE: usize = 5;

const RNG_SIZE: usize = SLIDING_WND_SIZE;

/// Builder object, processing streams of bytes to generate [`Tlshx`] objects.
///
/// You should never provide your own values for the generics, but instead use the pre-configured
/// types such as [`crate::TlshxBuilder256_1`] or [`crate::TlshxBuilder128_3`].
pub struct TlshxBuilder<
    const EFF_BUCKETS: usize,
    const TLSH_CHECKSUM_LEN: usize,
    const CODE_SIZE: usize,
    const TLSH_STRING_LEN_REQ: usize,
    const MIN_DATA_LENGTH: usize,
> {
    a_bucket: [u32; BUCKETS],
    slide_window: [u8; SLIDING_WND_SIZE],
    checksum: [u8; TLSH_CHECKSUM_LEN],
    data_len: usize,
}

impl<
        const EFF_BUCKETS: usize,
        const TLSH_CHECKSUM_LEN: usize,
        const CODE_SIZE: usize,
        const TLSH_STRING_LEN_REQ: usize,
        const MIN_DATA_LENGTH: usize,
    > Default
    for TlshxBuilder<
        EFF_BUCKETS,
        TLSH_CHECKSUM_LEN,
        CODE_SIZE,
        TLSH_STRING_LEN_REQ,
        MIN_DATA_LENGTH,
    >
{
    fn default() -> Self {
        Self::new()
    }
}

impl<
        const EFF_BUCKETS: usize,
        const TLSH_CHECKSUM_LEN: usize,
        const CODE_SIZE: usize,
        const TLSH_STRING_LEN_REQ: usize,
        const MIN_DATA_LENGTH: usize,
    >
    TlshxBuilder<EFF_BUCKETS, TLSH_CHECKSUM_LEN, CODE_SIZE, TLSH_STRING_LEN_REQ, MIN_DATA_LENGTH>
{
    /// Create a new TLSHX builder.
    pub fn new() -> Self {
        Self {
            a_bucket: [0; BUCKETS],
            slide_window: [0; SLIDING_WND_SIZE],
            checksum: [0; TLSH_CHECKSUM_LEN],
            data_len: 0,
        }
    }

    /// Generate a [`Tlshx`] object from a given byte slice.
    ///
    /// This is a shorthand for building a [`Tlshx`] object from a single
    /// byte slice, it is equivalent to:
    ///
    /// ```
    /// let data = b"Lorem ipsum dolor sit amet, consectetur adipiscing elit";
    /// let tlsh = tlsh2::TlshxDefaultBuilder::build_from(data);
    /// // equivalent to
    /// let mut builder = tlsh2::TlshxDefaultBuilder::new();
    /// builder.update(data);
    /// let tlsh = builder.build();
    /// ```
    pub fn build_from(
        data: &[u8],
    ) -> Option<Tlshx<TLSH_CHECKSUM_LEN, TLSH_STRING_LEN_REQ, CODE_SIZE>> {
        let mut builder = Self::new();
        builder.update(data);
        builder.build()
    }

    /// Add bytes into the builder.
    pub fn update(&mut self, data: &[u8]) {
        // TODO: TLSH_OPTION_THREADED | TLSH_OPTION_PRIVATE

        let mut j = self.data_len % RNG_SIZE;
        let mut fed_len = self.data_len;

        for b in data {
            self.slide_window[j] = *b;

            if fed_len >= 4 {
                let j_1 = (j + RNG_SIZE - 1) % RNG_SIZE;
                let j_2 = (j + RNG_SIZE - 2) % RNG_SIZE;
                let j_3 = (j + RNG_SIZE - 3) % RNG_SIZE;
                let j_4 = (j + RNG_SIZE - 4) % RNG_SIZE;

                for k in 0..TLSH_CHECKSUM_LEN {
                    if k == 0 {
                        self.checksum[k] = fast_b_mapping::<EFF_BUCKETS>(
                            1,
                            self.slide_window[j],
                            self.slide_window[j_1],
                            self.checksum[k],
                        );
                    } else {
                        self.checksum[k] = b_mapping(
                            self.checksum[k - 1],
                            self.slide_window[j],
                            self.slide_window[j_1],
                            self.checksum[k],
                        );
                    }
                }

                let r = fast_b_mapping::<EFF_BUCKETS>(
                    49,
                    self.slide_window[j],
                    self.slide_window[j_1],
                    self.slide_window[j_2],
                );
                self.a_bucket[usize::from(r)] += 1;
                let r = fast_b_mapping::<EFF_BUCKETS>(
                    12,
                    self.slide_window[j],
                    self.slide_window[j_1],
                    self.slide_window[j_3],
                );
                self.a_bucket[usize::from(r)] += 1;
                let r = fast_b_mapping::<EFF_BUCKETS>(
                    178,
                    self.slide_window[j],
                    self.slide_window[j_2],
                    self.slide_window[j_3],
                );
                self.a_bucket[usize::from(r)] += 1;
                let r = fast_b_mapping::<EFF_BUCKETS>(
                    166,
                    self.slide_window[j],
                    self.slide_window[j_2],
                    self.slide_window[j_4],
                );
                self.a_bucket[usize::from(r)] += 1;
                let r = fast_b_mapping::<EFF_BUCKETS>(
                    84,
                    self.slide_window[j],
                    self.slide_window[j_1],
                    self.slide_window[j_4],
                );
                self.a_bucket[usize::from(r)] += 1;
                let r = fast_b_mapping::<EFF_BUCKETS>(
                    230,
                    self.slide_window[j],
                    self.slide_window[j_3],
                    self.slide_window[j_4],
                );
                self.a_bucket[usize::from(r)] += 1;
            }
            fed_len += 1;
            j = (j + 1) % RNG_SIZE;
        }

        self.data_len += data.len();
    }

    /// Generate a [`Tlshx`] object, or None if the object is not valid.
    pub fn build(&self) -> Option<Tlshx<TLSH_CHECKSUM_LEN, TLSH_STRING_LEN_REQ, CODE_SIZE>> {
        if self.data_len < MIN_DATA_LENGTH {
            return None;
        }

        let (q1, q2) = get_tertiles::<EFF_BUCKETS>(&self.a_bucket);
        // issue #79 - divide by 0 if q2 == 0
        if q2 == 0 {
            return None;
        }

        // buckets must be more than 50% non-zero
        let nonzero = self
            .a_bucket
            .iter()
            .take(CODE_SIZE * 5)
            .filter(|v| **v > 0)
            .count();
        // TODO: Special case EFF_BUCKETS == 48
        if nonzero * 2 <= 5 * CODE_SIZE {
            return None;
        }

        let mut code: [u8; CODE_SIZE] = [0; CODE_SIZE];
        for (i, slice) in self.a_bucket.chunks(5).take(CODE_SIZE).enumerate() {
            let mut h = 0_u8;
            for (j, k) in slice.iter().enumerate() {
                if q2 < *k {
                    h += 2 * 3u8.pow(j as u32);
                } else if q1 < *k {
                    h += 1 * 3u8.pow(j as u32);
                }
            }
            code[i] = h;
        }

        let lvalue = l_capturing(self.data_len as u32);
        let q1_ratio = (((((q1 * 100) as f32) / (q2 as f32)) as u32) % 16) as u8;

        Some(Tlshx {
            lvalue,
            q1_ratio,
            checksum: self.checksum,
            code,
        })
    }
}

/// TLSHX object, from which a hash or a distance can be computed.
pub struct Tlshx<
    const TLSH_CHECKSUM_LEN: usize,
    const TLSH_STRING_LEN_REQ: usize,
    const CODE_SIZE: usize,
> {
    lvalue: u8,
    q1_ratio: u8,
    checksum: [u8; TLSH_CHECKSUM_LEN],
    code: [u8; CODE_SIZE],
}

impl<const TLSH_CHECKSUM_LEN: usize, const TLSH_STRING_LEN_REQ: usize, const CODE_SIZE: usize>
    Tlshx<TLSH_CHECKSUM_LEN, TLSH_STRING_LEN_REQ, CODE_SIZE>
{
    /// Compute the hash of a TLSHX.
    ///
    /// The hash is always prefixed by `TX` (`showvers=X` in the original TLSH version).
    /// This is due to the no_std implementation and the need to have a fixed-length result.
    /// Use a subslice on the result if you don't need this prefix.
    ///
    /// ```
    /// let data = b"Lorem ipsum dolor sit amet, consectetur adipiscing elit";
    /// let tlsh = tlsh2::TlshxDefaultBuilder::build_from(data)
    ///     .expect("should have generated a TLSHX");
    /// assert_eq!(
    ///     tlsh.hash().as_slice(),
    ///     b"TX2D9020092BA51B3F04A30015330A5200EC7F6C295154092A540057DC005A011B360001",
    /// );
    /// ```
    pub fn hash(&self) -> [u8; TLSH_STRING_LEN_REQ] {
        let mut hash = [0; TLSH_STRING_LEN_REQ];

        hash[0] = b'T';
        hash[1] = b'X';
        let mut i = 2;

        for k in &self.checksum {
            to_hex(&mut hash, &mut i, swap_byte(*k));
        }
        to_hex(&mut hash, &mut i, swap_byte(self.lvalue));

        let qb = self.q1_ratio << 4;
        to_hex(&mut hash, &mut i, qb);

        for c in self.code.iter().rev() {
            to_hex(&mut hash, &mut i, *c);
        }

        hash
    }

    /// Compute the difference between two TLSHX.
    ///
    /// The len_diff parameter specifies if the file length is to be included in
    /// the difference calculation (len_diff=true) or if it is to be excluded
    /// (len_diff=false).
    ///
    /// In general, the length should be considered in the difference calculation,
    /// but there could be applications where a part of the adversarial activity
    /// might be to add a lot of content.
    /// For example to add 1 million zero bytes at the end of a file. In that case,
    /// the caller would want to exclude the length from the calculation.
    ///
    /// ```
    /// let data1 = b"Lorem ipsum dolor sit amet, consectetur adipiscing elit";
    /// let tlsh1 = tlsh2::TlshxDefaultBuilder::build_from(data1)
    ///     .expect("should have generated a TLSHX");
    /// let data2 = b"Duis aute irure dolor in reprehenderit in voluptate velit \
    ///     esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat \
    ///     cupidatat non proident, sunt in culpa qui officia";
    /// let tlsh2 = tlsh2::TlshxDefaultBuilder::build_from(data2)
    ///     .expect("should have generated a TLSHX");
    ///
    /// assert_eq!(tlsh1.diff(&tlsh2, false), 232);
    /// assert_eq!(tlsh1.diff(&tlsh2, true), 268);
    /// ```
    #[cfg(feature = "diff")]
    pub fn diff(&self, other: &Self, len_diff: bool) -> i32 {
        use crate::util::{hx_distance, mod_diff};

        const LENGTH_MULT: i32 = 12;
        const QRATIO_MULT: i32 = 12;
        const RANGE_LVALUE: u32 = 256;
        const RANGE_QRATIO: u32 = 16;

        let mut diff;
        if len_diff {
            let ldiff = mod_diff(self.lvalue, other.lvalue, RANGE_LVALUE);
            if ldiff == 0 {
                diff = 0;
            } else if ldiff == 1 {
                diff = 1;
            } else {
                diff = ldiff * LENGTH_MULT;
            }
        } else {
            diff = 0;
        }

        let q1diff = mod_diff(self.q1_ratio, other.q1_ratio, RANGE_QRATIO);
        if q1diff <= 1 {
            diff += q1diff;
        } else {
            diff += (q1diff - 1) * QRATIO_MULT;
        }

        for (a, b) in self.checksum.iter().zip(other.checksum.iter()) {
            if a != b {
                diff += 1;
                break;
            }
        }

        diff += hx_distance(&self.code, &other.code);

        diff
    }

    fn from_hash(s: &[u8]) -> Option<Self> {
        if s.len() != TLSH_STRING_LEN_REQ || s[0] != b'T' || s[1] != b'X' {
            return None;
        }

        let mut i = 2;

        let mut checksum = [0; TLSH_CHECKSUM_LEN];
        for k in &mut checksum {
            *k = swap_byte(from_hex(s, &mut i)?);
        }

        let lvalue = swap_byte(from_hex(s, &mut i)?);
        let qb = from_hex(s, &mut i)?;
        let q1_ratio = qb >> 4;

        let mut code = [0; CODE_SIZE];
        for c in code.iter_mut().rev() {
            *c = from_hex(s, &mut i)?;
            if *c > 242 {
                return None;
            }
        }

        Some(Self {
            lvalue,
            q1_ratio,
            checksum,
            code,
        })
    }
}

use crate::tlsh::{from_hex, to_hex, ParseError};

/// Parse a hash string and build the corresponding `Tlshx` object.
impl<const TLSH_CHECKSUM_LEN: usize, const TLSH_STRING_LEN_REQ: usize, const CODE_SIZE: usize>
    FromStr for Tlshx<TLSH_CHECKSUM_LEN, TLSH_STRING_LEN_REQ, CODE_SIZE>
{
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hash(s.as_bytes()).ok_or(ParseError)
    }
}
