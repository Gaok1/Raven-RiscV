use falcon::memory::Memory;


mod falcon;

fn main() {
    
    let mut mem = Memory::new();
    let address = 0x2;
    let val : i32 = 23;
    printBinary(val as u32);
    mem.write_word(address, val as u32);
    let read_val = mem.read_word(address);
    printBinary(read_val);
    println!("Valor lido: {}", read_val as i32);
    let val2 = mem.read_byte(address);
    printBinary(val2 as u32);
    println!("Valor lido: {}", val2 as i32);

}


fn printBinary(value: u32) {
    for i in (0..32).rev() {
        if (i %8)==0 && i!=0{
            print!(" ");
        }
        print!("{}", (value >> i) & 1);
    }
    println!();
}