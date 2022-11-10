#![feature(test)]

extern crate test;

use solana_program::big_mod_exp::big_mod_exp;

use rand::Rng;

#[bench]
fn big_mod_exp_test(b: &mut Bencher) {

    #[serde(rename_all = "PascalCase")]
    #[derive(serde::Deserialize)]
    struct TestCase{
        base: String,
        exponent: String,
        modulus: String,
        expected: String,
    }

    let test_data = r#"[
        {
            "Base":     "1111111111111111111111111111111111111111111111111111111111111111",
            "Exponent": "1111111111111111111111111111111111111111111111111111111111111111",
            "Modulus":  "111111111111111111111111111111111111111111111111111111111111110A",
            "Expected": "0A7074864588D6847F33A168209E516F60005A0CEC3F33AAF70E8002FE964BCD"
        },
        {
            "Base":     "2222222222222222222222222222222222222222222222222222222222222222",
            "Exponent": "2222222222222222222222222222222222222222222222222222222222222222",
            "Modulus":  "1111111111111111111111111111111111111111111111111111111111111111",
            "Expected": "00"
        },
        {
            "Base":     "3333333333333333333333333333333333333333333333333333333333333333",
            "Exponent": "3333333333333333333333333333333333333333333333333333333333333333",
            "Modulus":  "2222222222222222222222222222222222222222222222222222222222222222",
            "Expected": "1111111111111111111111111111111111111111111111111111111111111111"
        },
        {
            "Base":     "9874231472317432847923174392874918237439287492374932871937289719",
            "Exponent": "0948403985401232889438579475812347232099080051356165126166266222",
            "Modulus":  "25532321a214321423124212222224222b242222222222222222222222222444",
            "Expected": "220ECE1C42624E98AEE7EB86578B2FE5C4855DFFACCB43CCBB708A3AB37F184D"
        },
        {
            "Base":     "3494396663463663636363662632666565656456646566786786676786768766",
            "Exponent": "2324324333246536456354655645656616169896565698987033121934984955",
            "Modulus":  "0218305479243590485092843590249879879842313131156656565565656566",
            "Expected": "012F2865E8B9E79B645FCE3A9E04156483AE1F9833F6BFCF86FCA38FC2D5BEF0"
        },
        {
            "Base":     "0000000000000000000000000000000000000000000000000000000000000005",
            "Exponent": "0000000000000000000000000000000000000000000000000000000000000002",
            "Modulus":  "0000000000000000000000000000000000000000000000000000000000000007",
            "Expected": "0000000000000000000000000000000000000000000000000000000000000004"
        },
        {
            "Base":     "0000000000000000000000000000000000000000000000000000000000000019",
            "Exponent": "0000000000000000000000000000000000000000000000000000000000000019",
            "Modulus":  "0000000000000000000000000000000000000000000000000000000000000064",
            "Expected": "0000000000000000000000000000000000000000000000000000000000000019"
        }
    ]"#;

    let test_cases: Vec<TestCase> = serde_json::from_str(test_data).unwrap();
    b.iter(|| {
        test_cases.iter().for_each(|test|{
            let base = array_bytes::hex2bytes_unchecked(&test.base);
            let exponent = array_bytes::hex2bytes_unchecked(&test.exponent);
            let modulus = array_bytes::hex2bytes_unchecked(&test.modulus);
            let expected = array_bytes::hex2bytes_unchecked(&test.expected);

            let result = big_mod_exp(
                base.as_slice(),exponent.as_slice(),modulus.as_slice(),
            );
            assert_eq!(result, expected);
        });
    });
}


use criterion::{criterion_group, criterion_main, Criterion};


criterion_group!(benches,
    big_mod_exp_test_rnd
);
criterion_main!(benches);


fn big_mod_exp_test_rnd(c: &mut Criterion) {
    let mut rng = rand::thread_rng();

    for len in (32..=128).step_by(32){

        let base   = vec![0_u8; len].iter().map(|_| {rng.gen()}).collect::<Vec<u8>>();
        let exponent   = vec![0_u8; len].iter().map(|_| {rng.gen()}).collect::<Vec<u8>>();
        let modulus   = vec![0_u8; len].iter().map(|_| {rng.gen()}).collect::<Vec<u8>>();


        c.bench_function("big_mod_exp rnd", |b| b.iter(||
            big_mod_exp(
                base.as_slice(),exponent.as_slice(),modulus.as_slice(),
            )
        ));
    }
}
