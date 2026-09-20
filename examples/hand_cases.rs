use bitmatrix::{BitMatrix, RunMesh};

fn bits_from_rows(rows: &[&[u8]]) -> BitMatrix {
    let mut bits = BitMatrix::new();
    for (y, row) in rows.iter().enumerate() {
        for (x, &c) in row.iter().enumerate() {
            if c == 1 {
                bits.set(x as u8, y as u8);
            }
        }
    }
    bits
}

fn main() {
    let cases: [(&str, u32, Vec<&[u8]>); 2] = [
        (
            "4x4 adversarial",
            3,
            vec![&[1, 1, 0, 0][..], &[0, 1, 1, 1][..], &[1, 1, 1, 0][..], &[0, 0, 0, 0][..]],
        ),
        (
            "worked 8x8 example",
            10,
            vec![
                &[1, 1, 1, 1, 0, 1, 1, 1][..],
                &[1, 0, 0, 1, 0, 1, 1, 1][..],
                &[1, 1, 1, 1, 0, 1, 1, 1][..],
                &[0, 0, 0, 1, 0, 0, 0, 1][..],
                &[0, 0, 0, 1, 1, 0, 0, 1][..],
                &[0, 0, 0, 1, 1, 1, 1, 1][..],
                &[1, 1, 1, 1, 1, 1, 1, 1][..],
                &[1, 1, 0, 1, 1, 1, 1, 1][..],
            ],
        ),
    ];

    for (name, opt, rows) in cases {
        let bits = bits_from_rows(&rows);
        println!(
            "{name}: optimum {opt}, shipped {}, largest+maxarea {}, largest+whole {}",
            RunMesh::from_bit_matrix(&bits).rects().len(),
            RunMesh::largest_first(&bits, 0).rects().len(),
            RunMesh::largest_first(&bits, 1 << 20).rects().len(),
        );
    }
}
