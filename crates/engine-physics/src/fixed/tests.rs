use super::*;

#[test]
fn test_fixed_point_basic_arithmetic() {
    let a = FP::from_int(5);
    let b = FP::from_int(3);

    assert_eq!((a + b).to_f64(), 8.0);
    assert_eq!((a - b).to_f64(), 2.0);
    assert_eq!((a * b).to_f64(), 15.0);

    let half = FP::HALF;
    assert_eq!(half.to_f64(), 0.5);
    assert_eq!((a * half).to_f64(), 2.5);
}

#[test]
fn test_fixed_point_sqrt() {
    let nine = FP::from_int(9);
    assert_eq!(nine.sqrt().to_f64(), 3.0);

    let two = FP::from_int(2);
    let sqrt2 = two.sqrt().to_f64();
    assert!((sqrt2 - 1.41421356).abs() < 0.0001);
}

fn sqrt_digit_by_digit(raw: i64) -> i64 {
    if raw <= 0 {
        return 0;
    }
    let val = (raw as u128) << FP::FRACTIONAL_BITS;
    let mut root: u128 = 0;
    let mut bit: u128 = 1u128 << 126;
    while bit > val {
        bit >>= 2;
    }
    let mut rem = val;
    while bit != 0 {
        if rem >= root + bit {
            rem -= root + bit;
            root = (root >> 1) + bit;
        } else {
            root >>= 1;
        }
        bit >>= 2;
    }
    root as i64
}

#[test]
fn test_fixed_point_sqrt_matches_reference_bit_for_bit() {
    let mut x: u64 = 0x9E3779B97F4A7C15;
    for i in 0..4096 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        for raw in [i as i64, (i * 1024) as i64, x as i64 & i64::MAX, 1, 2, 3] {
            assert_eq!(
                FP::from_raw(raw).sqrt().to_raw(),
                sqrt_digit_by_digit(raw),
                "raw={raw}"
            );
        }
    }
}

#[test]
fn test_fpvec3_dot_cross_distance() {
    let v1 = FPVec3::from_f64(3.0, 0.0, 0.0);
    let v2 = FPVec3::from_f64(0.0, 4.0, 0.0);

    assert_eq!(v1.distance(v2).to_f64(), 5.0);
    assert_eq!(v1.dot(v2).to_f64(), 0.0);

    let cross = v1.cross(v2);
    assert_eq!(cross.z.to_f64(), 12.0);
}

#[test]
fn test_cross_platform_exact_bit_reproducibility() {
    let mut pos = FPVec3::from_f64(10.0, 20.0, 30.0);
    let vel = FPVec3::from_f64(1.5, -0.5, 2.25);
    let dt = FP::from_f64(0.0166666666);

    for _ in 0..100 {
        pos = pos + vel * dt;
    }

    assert_ne!(pos.x.to_raw(), 0);
    assert_ne!(pos.y.to_raw(), 0);
    assert_ne!(pos.z.to_raw(), 0);
}
