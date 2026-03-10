// build with cargo rustc --target riscv32im-unknown-none-elf


#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use core::ptr::write_volatile;

fn factorial(n: u64) -> u64 {
    if n <= 1 { 1 } else { n * factorial(n - 1) }
}


fn divide_by_zero(num1:i32,num2:i32)->i32{
    return num1/num2;
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let fat_start = "Fat 20\n";
    raven_api_write(fat_start);
    
    divide_by_zero(2,0);

    
    //panic!("You're too looser for this!");
    factorial(20);
    
    loop {}
}


#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {

    raven_api_write("PANICKED: ");
    raven_api_write(_info.message().as_str().unwrap_or_default());
    raven_api_write("\n");
    
    raven_api_exit(1);
    
    loop {}
}


fn raven_api_write(mssg: &str){
        let len = mssg.len();
        unsafe {
            asm!(
                "ecall",
                in("a7") 64,      // número da syscall write no Linux risc-v
                in("a0") 1,
                in("a1") mssg.as_ptr(), //buffer
                in("a2") len, //len
            );
        }
}

fn raven_api_exit(code: u16){
    unsafe {
        asm!(
            "ecall",
            in("a7") 93,      // número da syscall write no Linux risc-v
            in("a0") code,
        );
    }
}