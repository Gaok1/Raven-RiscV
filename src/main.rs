

fn main() {
    
}

// let pointer = 5;
// let mut memory: [u8; 1024] = [0; 1024];

// // Simula leitura de um inteiro da memória
// let slice = &memory[pointer..pointer + 4];
// let mut int = u32::from_be_bytes(slice.try_into().unwrap());

// // Modifica o valor
// int = 200;

// // Escreve de volta na memória
// let bytes = int.to_be_bytes();
// memory[pointer..pointer + 4].copy_from_slice(&bytes);

// // Verifica se o valor foi alterado
// let slice = &memory[pointer..pointer + 4];
// let int = u32::from_be_bytes(slice.try_into().unwrap());
// println!("{}", int);  // Deve imprimir 200
