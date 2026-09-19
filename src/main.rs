use bitmatrix::BitMatrix;

fn print_matrix(m: &BitMatrix) {
    for y in 0..bitmatrix::HEIGHT {
        let mut line = String::with_capacity(bitmatrix::WIDTH);
        for x in 0..bitmatrix::WIDTH {
            line.push(if m.get(x, y) { '#' } else { '.' });
        }
        println!("{line}");
    }
}

fn main() {
    let mut m = BitMatrix::new();

    m.set_rect(10, 10, 40, 30);
    m.set_circle(180, 180, 25);
    m.unset_rect(20, 15, 30, 25);
    m.unset_circle(180, 180, 8);

    print_matrix(&m);
    println!("bits set: {}", m.count_set());

    m.reset();
    println!("bits set after reset: {}", m.count_set());
}
