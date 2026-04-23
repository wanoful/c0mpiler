use std::ops::{Add, BitAnd, BitOr, BitXor, Mul, Sub};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]

pub struct CoreInt {
    pub value: u64,
    pub bit_width: u8,
}

impl CoreInt {
    pub fn new(value: u64, bit_width: u8) -> Self {
        assert!(
            (1..=64).contains(&bit_width),
            "CoreInt bit width must be in 1..=64"
        );
        Self {
            value: value & Self::mask_for(bit_width),
            bit_width,
        }
    }

    fn mask_for(bit_width: u8) -> u64 {
        if bit_width >= 64 {
            u64::MAX
        } else {
            (1u64 << bit_width) - 1
        }
    }

    fn mask(&self) -> u64 {
        Self::mask_for(self.bit_width)
    }

    pub fn from_signed(value: i64, bit_width: u8) -> Self {
        Self::new(value as u64, bit_width)
    }

    pub fn as_u64(&self) -> u64 {
        self.value
    }

    pub fn as_i64(&self) -> i64 {
        if self.bit_width == 64 {
            self.value as i64
        } else {
            let sign_mask = 1u64 << (self.bit_width - 1);
            if (self.value & sign_mask) != 0 {
                (self.value | !self.mask()) as i64
            } else {
                self.value as i64
            }
        }
    }

    pub fn trunc_to(self, new_bit_width: u8) -> Self {
        assert!(
            new_bit_width <= self.bit_width,
            "trunc target width must be <= source width"
        );
        Self::new(self.value, new_bit_width)
    }

    pub fn zero_extend(self, new_bit_width: u8) -> Self {
        assert!(
            new_bit_width >= self.bit_width,
            "zext target width must be >= source width"
        );
        Self::new(self.value, new_bit_width)
    }

    pub fn sign_extend(self, new_bit_width: u8) -> Self {
        assert!(
            new_bit_width >= self.bit_width,
            "sext target width must be >= source width"
        );

        if new_bit_width == self.bit_width {
            return self;
        }

        let sign_set = ((self.value >> (self.bit_width - 1)) & 1) == 1;
        let mut new_value = self.value;
        if sign_set {
            new_value |= !self.mask();
        }
        Self::new(new_value, new_bit_width)
    }

    fn assert_same_bit_width(&self, rhs: &Self) {
        assert_eq!(
            self.bit_width, rhs.bit_width,
            "CoreInt operands must have the same bit width"
        );
    }

    pub fn wrapping_add(self, rhs: Self) -> Self {
        self.assert_same_bit_width(&rhs);
        Self::new(self.value.wrapping_add(rhs.value), self.bit_width)
    }

    pub fn wrapping_sub(self, rhs: Self) -> Self {
        self.assert_same_bit_width(&rhs);
        Self::new(self.value.wrapping_sub(rhs.value), self.bit_width)
    }

    pub fn wrapping_mul(self, rhs: Self) -> Self {
        self.assert_same_bit_width(&rhs);
        Self::new(self.value.wrapping_mul(rhs.value), self.bit_width)
    }

    pub fn checked_udiv(self, rhs: Self) -> Option<Self> {
        self.assert_same_bit_width(&rhs);
        self.value
            .checked_div(rhs.value)
            .map(|value| Self::new(value, self.bit_width))
    }

    pub fn checked_sdiv(self, rhs: Self) -> Option<Self> {
        self.assert_same_bit_width(&rhs);

        let lhs = self.as_i64();
        let rhs = rhs.as_i64();
        lhs.checked_div(rhs)
            .map(|value| Self::from_signed(value, self.bit_width))
    }

    pub fn checked_urem(self, rhs: Self) -> Option<Self> {
        self.assert_same_bit_width(&rhs);
        self.value
            .checked_rem(rhs.value)
            .map(|value| Self::new(value, self.bit_width))
    }

    pub fn checked_srem(self, rhs: Self) -> Option<Self> {
        self.assert_same_bit_width(&rhs);

        let lhs = self.as_i64();
        let rhs = rhs.as_i64();
        lhs.checked_rem(rhs)
            .map(|value| Self::from_signed(value, self.bit_width))
    }

    pub fn checked_shl(self, rhs: Self) -> Option<Self> {
        self.assert_same_bit_width(&rhs);

        if rhs.value >= self.bit_width as u64 {
            return None;
        }

        Some(Self::new(self.value.wrapping_shl(rhs.value as u32), self.bit_width))
    }

    pub fn checked_lshr(self, rhs: Self) -> Option<Self> {
        self.assert_same_bit_width(&rhs);

        if rhs.value >= self.bit_width as u64 {
            return None;
        }

        Some(Self::new(self.value.wrapping_shr(rhs.value as u32), self.bit_width))
    }

    pub fn checked_ashr(self, rhs: Self) -> Option<Self> {
        self.assert_same_bit_width(&rhs);

        if rhs.value >= self.bit_width as u64 {
            return None;
        }

        let value = self.as_i64().wrapping_shr(rhs.value as u32);
        Some(Self::from_signed(value, self.bit_width))
    }

    pub fn bitand(self, rhs: Self) -> Self {
        self.assert_same_bit_width(&rhs);
        Self::new(self.value & rhs.value, self.bit_width)
    }

    pub fn bitor(self, rhs: Self) -> Self {
        self.assert_same_bit_width(&rhs);
        Self::new(self.value | rhs.value, self.bit_width)
    }

    pub fn bitxor(self, rhs: Self) -> Self {
        self.assert_same_bit_width(&rhs);
        Self::new(self.value ^ rhs.value, self.bit_width)
    }

    pub fn cmp_eq(self, rhs: Self) -> bool {
        self.assert_same_bit_width(&rhs);
        self.value == rhs.value
    }

    pub fn cmp_ne(self, rhs: Self) -> bool {
        !self.cmp_eq(rhs)
    }

    pub fn cmp_ugt(self, rhs: Self) -> bool {
        self.assert_same_bit_width(&rhs);
        self.value > rhs.value
    }

    pub fn cmp_uge(self, rhs: Self) -> bool {
        self.assert_same_bit_width(&rhs);
        self.value >= rhs.value
    }

    pub fn cmp_ult(self, rhs: Self) -> bool {
        self.assert_same_bit_width(&rhs);
        self.value < rhs.value
    }

    pub fn cmp_ule(self, rhs: Self) -> bool {
        self.assert_same_bit_width(&rhs);
        self.value <= rhs.value
    }

    pub fn cmp_sgt(self, rhs: Self) -> bool {
        self.assert_same_bit_width(&rhs);
        self.as_i64() > rhs.as_i64()
    }

    pub fn cmp_sge(self, rhs: Self) -> bool {
        self.assert_same_bit_width(&rhs);
        self.as_i64() >= rhs.as_i64()
    }

    pub fn cmp_slt(self, rhs: Self) -> bool {
        self.assert_same_bit_width(&rhs);
        self.as_i64() < rhs.as_i64()
    }

    pub fn cmp_sle(self, rhs: Self) -> bool {
        self.assert_same_bit_width(&rhs);
        self.as_i64() <= rhs.as_i64()
    }

    pub fn to_const_i64(self) -> i64 {
        self.as_i64()
    }
}

impl Add for CoreInt {
    type Output = CoreInt;

    fn add(self, rhs: Self) -> Self::Output {
        self.wrapping_add(rhs)
    }
}

impl Sub for CoreInt {
    type Output = CoreInt;

    fn sub(self, rhs: Self) -> Self::Output {
        self.wrapping_sub(rhs)
    }
}

impl Mul for CoreInt {
    type Output = CoreInt;

    fn mul(self, rhs: Self) -> Self::Output {
        self.wrapping_mul(rhs)
    }
}

impl BitAnd for CoreInt {
    type Output = CoreInt;

    fn bitand(self, rhs: Self) -> Self::Output {
        self.bitand(rhs)
    }
}

impl BitOr for CoreInt {
    type Output = CoreInt;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.bitor(rhs)
    }
}

impl BitXor for CoreInt {
    type Output = CoreInt;

    fn bitxor(self, rhs: Self) -> Self::Output {
        self.bitxor(rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::CoreInt;

    #[test]
    fn fixed_width_wraps_on_create_and_add() {
        let x = CoreInt::new(0xff, 8);
        assert_eq!(x.as_u64(), 0xff);
        assert_eq!((x + CoreInt::new(1, 8)).as_u64(), 0);
    }

    #[test]
    fn trunc_zext_sext_follow_bit_semantics() {
        let x = CoreInt::new(0b1111_1101, 8);
        assert_eq!(x.clone().trunc_to(4).as_u64(), 0b1101);
        assert_eq!(x.clone().trunc_to(4).zero_extend(8).as_u64(), 0b0000_1101);
        assert_eq!(x.trunc_to(4).sign_extend(8).as_u64(), 0b1111_1101);
    }

    #[test]
    fn shift_out_of_range_returns_none() {
        let x = CoreInt::new(1, 8);
        assert!(x.clone().checked_shl(CoreInt::new(8, 8)).is_none());
        assert!(x.clone().checked_lshr(CoreInt::new(8, 8)).is_none());
        assert!(x.checked_ashr(CoreInt::new(8, 8)).is_none());
    }

    #[test]
    fn signed_unsigned_division_behaves_as_expected() {
        let minus_one_i8 = CoreInt::from_signed(-1, 8);
        let two_i8 = CoreInt::new(2, 8);
        assert_eq!(
            minus_one_i8
                .clone()
                .checked_sdiv(two_i8.clone())
                .unwrap()
                .as_i64(),
            0
        );
        assert_eq!(minus_one_i8.clone().checked_udiv(two_i8).unwrap().as_u64(), 127);
        assert!(minus_one_i8.clone().checked_sdiv(CoreInt::new(0, 8)).is_none());
        assert!(minus_one_i8.checked_udiv(CoreInt::new(0, 8)).is_none());
    }
}
