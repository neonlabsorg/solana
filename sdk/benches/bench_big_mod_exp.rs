#![feature(test)]

extern crate test;

use solana_program::big_mod_exp::big_mod_exp;

// use rand::Rng;

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
            "Expected": "0000000000000000000000000000000000000000000000000000000000000000"
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
    big_mod_exp_test_rsa
);
criterion_main!(benches);


fn big_mod_exp_test_rsa(c: &mut Criterion) {
    let rsa = [
        "D0575439",
        "BAB298F775C1CF9F",
        "A9CBDF29F6E078A40C47716B429A2BAF",
        "D7774B0C36C328A124C4C895A6000B7AAD1434F77D406AB67DD322DBC46318AB",
        "A2BAC741546DB79EDE3999899EFFC967D9755D7AE570BB7ABFFC3F8CD54F08F3B4F62C051CAE5E192DBECCE111F48D90CF59F50DF6CA001C2B17D73BF43E967F",
        "BE667570AA559AD38CA9BB83C22689CB735DB771A1DEA71195AC9FF2AF627A30AF8B3219C9A912E8869376F66B94FCC038A0C07AD907FB8E7918743A2F44E0FA3CB87D0F61C95B58E29E48EDC5E987B5E3A4EE67843F8B4B9F763E4E585105F42E8CC71FE948205ECF1C587ED87C0224269E790620D556CA11D9B0C8BAE8ECD1",
        "DE4EE0E47828D37DDACBAC953DDE253FEC380509173E5323539145757D218BE8223F62CDE0C888A9A7ACD533965D52660E1CB366ACBD7271F9AA836CDC142022688BEC48355CCCC71EAE194C221788D63E7587D220ECCDB6D6890097EAFBA7AA846C16485EF4A02C1E07168374C24CC3B9FE00C33A29AF300415448A1E14F41DB41A89BAD2883A6107E492B3D07EDF5CAF5C8BA31E1B861D26B286356A34A874A6EF65B09B4B3ED586C1DD77EB751D447BCA2C9DCA73B6372B78B207DD5C05443D2BD13706A3ACF5D71BF37ABF6888593C5CB1766E9B662AC47E424D745BDABBED043E7AAE2E293F9247867FCCCF11DFECCBBD08435DC89AD60AA164CE262333",
    ];
    let bit_size = [32, 64, 128, 256, 512, 1024, 2048];

    for (bits, modulus) in bit_size.iter().zip(rsa){

        let len= bits/8;

        let base   = vec![1u8; len];
        let exponent   = vec![1u8; len];
        let modulus = array_bytes::hex2bytes_unchecked(&modulus);

        c.bench_function("big_mod_exp rnd", |b| b.iter(||
            big_mod_exp(
                base.as_slice(),exponent.as_slice(),modulus.as_slice(),
            )
        ));
    }
}
// fn big_mod_exp_test_rnd(c: &mut Criterion) {
//     let mut rng = rand::thread_rng();
//
//     for len in (32..=128).step_by(32){
//
//         let base   = vec![0_u8; len].iter().map(|_| {rng.gen()}).collect::<Vec<u8>>();
//         let exponent   = vec![0_u8; len].iter().map(|_| {rng.gen()}).collect::<Vec<u8>>();
//         let modulus   = vec![0_u8; len].iter().map(|_| {rng.gen()}).collect::<Vec<u8>>();
//
//
//         c.bench_function("big_mod_exp rnd", |b| b.iter(||
//             big_mod_exp(
//                 base.as_slice(),exponent.as_slice(),modulus.as_slice(),
//             )
//         ));
//     }
// }
