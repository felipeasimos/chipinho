#[cfg(target_arch="wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;

use crate::{constants::*, error::Error, instruction::Instruction, font::{FONT_SET, FONT_SIZE}};

#[cfg_attr(target_arch="wasm32", wasm_bindgen)]
#[cfg_attr(not(target_arch="wasm32"), repr(C))]
pub struct Emulator {
    pub program_counter: u16,
    pub index: u16,
    pub delay_timer: u8,
    pub sound_timer: u8,
    pub waiting_for_key: Option<usize>,
    pub last_random_u8: u8,
    pub stack_size: u16,
    pub registers: [u8; NUM_REGISTERS],
    pub stack: [u16; MAX_STACK_SIZE],
    pub memory: [u8; MEMORY_SIZE as usize],
    pub vram: [bool; DISPLAY_WIDTH as usize * DISPLAY_HEIGHT as usize],
}

#[cfg_attr(target_arch="wasm32", wasm_bindgen)]
impl Emulator {

    #[cfg_attr(not(target_arch="wasm32"), no_mangle)]
    pub fn new() -> Self {
        let mut emulator = Emulator {
            program_counter: PROGRAM_BEGIN_ADDR,
            index: 0,
            delay_timer: 0,
            sound_timer: 0,
            waiting_for_key: None,
            last_random_u8: 123,
            stack_size: 0,
            registers: [0; NUM_REGISTERS],
            stack: [0; MAX_STACK_SIZE],
            memory: [0; MEMORY_SIZE as usize],
            vram: [false; DISPLAY_WIDTH as usize * DISPLAY_HEIGHT as usize],
        };
        // load fonts to memory
        emulator.memory
            .iter_mut()
            .skip(FONT_BEGIN_ADDR as usize)
            .take(FONT_SET.len())
            .zip(FONT_SET.iter())
            .for_each(|(byte, font_data)| *byte = *font_data);
        emulator
    }

    #[cfg_attr(not(target_arch="wasm32"), no_mangle)]
    pub extern "C" fn get_memory_ptr(&self) -> *const u8 {
        let u8_slice: &[u8] = unsafe {
            core::slice::from_raw_parts(self.memory.as_ptr() as *const u8, self.memory.len())
        };
        u8_slice.as_ptr()
    }

    #[cfg_attr(not(target_arch="wasm32"), no_mangle)]
    pub extern "C" fn get_vram_ptr(&self) -> *const u8 {
       // Convert the bool slice to a u8 slice
        let u8_slice: &[u8] = unsafe {
            core::slice::from_raw_parts(self.vram.as_ptr() as *const u8, self.vram.len())
        };

        // Return the pointer to the u8 slice
        u8_slice.as_ptr()
    }

    #[cfg_attr(not(target_arch="wasm32"), no_mangle)]
    pub extern "C" fn should_beep(&self) -> bool {
        self.sound_timer > 0
    }

    #[cfg_attr(not(target_arch="wasm32"), no_mangle)]
    pub extern "C" fn load_program(&mut self, program: &[u8]) -> Result<(), Error> {
        let max_program_length = (self.memory.len() as u16) - PROGRAM_BEGIN_ADDR;
        if (program.len() as u16) > max_program_length {
            return Err(Error::NotEnoughMemoryForProgram);
        }
        self.memory
            .iter_mut() // grab memory mutably
            .skip(PROGRAM_BEGIN_ADDR as usize) // skip to address where program will be written to
            .take(program.len()) // truncate to program size
            .zip(program)
            .for_each(|(memory_byte, program_byte)| *memory_byte = *program_byte);
        Ok(())
    }

    fn get_random_u8(&mut self) -> u8 {
        self.last_random_u8 = (RANDOM_MULTIPLIER * self.last_random_u8 + RANDOM_INCREMENT) % RANDOM_MODULE;
        self.last_random_u8
    }

    #[cfg_attr(not(target_arch="wasm32"), no_mangle)]
    pub fn get_opcode(&self) -> Result<Instruction, Error> {
        let first_byte : u8 = self.memory
            .get(self.program_counter as usize)
            .cloned()
            .ok_or_else(|| Error::OutOfBoundsMemoryAccess(self.program_counter))?;
        let second_byte : u8 = self.memory
            .get((self.program_counter + 1) as usize)
            .cloned()
            .ok_or_else(|| Error::OutOfBoundsMemoryAccess(self.program_counter))?;

        let raw_opcode = ((first_byte as u16) << 8) & (second_byte as u16);
        Instruction::parse(raw_opcode)
    }

    #[cfg_attr(not(target_arch="wasm32"), no_mangle)]
    pub extern "C" fn tick(&mut self, keypad: &[bool]) -> Result<(), Error> {
        match self.waiting_for_key {
            Some(key_index) => if keypad[key_index] {
                self.waiting_for_key = None;
            } else {
                return Ok(());
            },
            None => return Ok(()),
        };
        let opcode : Instruction = self.get_opcode()?;
        self.run_opcode(opcode, &keypad);
        if self.delay_timer > 0 {
            self.delay_timer -= 1;
        }
        if self.sound_timer > 0 {
            self.sound_timer -= 1;
        }
        Ok(())
    }

    fn run_opcode(&mut self, opcode : Instruction, keypad: &[bool]) -> () {
        match opcode {
            Instruction::Op0nnn(addr) => {
                self.program_counter = addr as u16;
            },
            Instruction::Op00E0 => {
                self.vram
                    .iter_mut()
                    .for_each(|pixel| *pixel = false);
                self.program_counter += 1;
            },
            Instruction::Op00EE => {
                self.program_counter = self.stack[self.stack_size as usize];
                self.stack_size -= 1
            },
            Instruction::Op1nnn(addr) => {
                self.program_counter = addr as u16;
            },
            Instruction::Op2nnn(addr) => {
                self.stack[self.stack_size as usize] = self.program_counter;
                self.stack_size += 1;
                self.program_counter = addr as u16;
            },
            Instruction::Op3xkk(register_index, value) => {
                if self.registers[register_index as usize] as u16 == value {
                    self.program_counter += 2;
                }
                self.program_counter += 1;
            },
            Instruction::Op4xkk(register_index, value) => {
                if self.registers[register_index as usize] as u16 != value {
                    self.program_counter += 2;
                }
                self.program_counter += 1;
            },
            Instruction::Op5xy0(register_index1, register_index2) => {
                if self.registers[register_index1 as usize] == self.registers[register_index2 as usize] {
                    self.program_counter += 2;
                }
                self.program_counter += 1;
            },
            Instruction::Op6xkk(register_index, value) => {
                self.registers[register_index as usize] = value as u8;
                self.program_counter += 1;
            },
            Instruction::Op7xkk(register_index, value) => {
                self.registers[register_index as usize] += value as u8;
                self.program_counter += 1;
            },
            Instruction::Op8xy0(register_index1, register_index2) => {
                self.registers[register_index1 as usize] = self.registers[register_index2 as usize];
                self.program_counter += 1;
            },
            Instruction::Op8xy1(register_index1, register_index2) => {
                self.registers[register_index1 as usize] |= self.registers[register_index2 as usize];
                self.program_counter += 1;
            },
            Instruction::Op8xy2(register_index1, register_index2) => {
                self.registers[register_index1 as usize] &= self.registers[register_index2 as usize];
                self.program_counter += 1;
            },
            Instruction::Op8xy3(register_index1, register_index2) => {
                self.registers[register_index1 as usize] ^= self.registers[register_index2 as usize];
                self.program_counter += 1;
            },
            Instruction::Op8xy4(register_index1, register_index2) => {
                let result : u16 = self.registers[register_index1 as usize] as u16 + self.registers[register_index2 as usize] as u16;
                self.registers[NUM_REGISTERS - 1] = if result > 255 { 1 } else { 0 };
                self.registers[register_index1 as usize] = result as u8;
                self.program_counter += 1;
            },
            Instruction::Op8xy5(register_index1, register_index2) => {
                let result : u16 = self.registers[register_index1 as usize] as u16 - self.registers[register_index2 as usize] as u16;
                self.registers[NUM_REGISTERS - 1] = if self.registers[register_index1 as usize] > self.registers[register_index2 as usize] { 1 } else { 0 };
                self.registers[register_index1 as usize] = result as u8;
                self.program_counter += 1;
            },
            Instruction::Op8xy6(register_index1, register_index2) => {
                self.registers[NUM_REGISTERS - 1] = if self.registers[register_index2 as usize] & 0x1 > 0 { 1 } else { 0 };
                self.registers[register_index2 as usize] = self.registers[register_index1 as usize] >> 1;
                self.program_counter += 1;
            },
            Instruction::Op8xy7(register_index1, register_index2) => {
                let result : u16 = self.registers[register_index2 as usize] as u16 - self.registers[register_index1 as usize] as u16;
                self.registers[NUM_REGISTERS - 1] = if self.registers[register_index2 as usize] > self.registers[register_index1 as usize] { 1 } else { 0 };
                self.registers[register_index1 as usize] = result as u8;
                self.program_counter += 1;
            },
            Instruction::Op8xyE(register_index1, register_index2) => {
                self.registers[NUM_REGISTERS - 1] = if self.registers[register_index1 as usize] & 0b1000_0000 > 0 { 1 } else { 0 };
                self.registers[register_index2 as usize] = self.registers[register_index1 as usize] << 1;
                self.program_counter += 1;
            },
            Instruction::Op9xy0(register_index1, register_index2) => {
                if self.registers[register_index1 as usize] != self.registers[register_index2 as usize] {
                    self.program_counter += 2;
                }
                self.program_counter += 1;
            },
            Instruction::OpAnnn(addr) => {
                self.index = addr as u16;
                self.program_counter += 1;
            },
            Instruction::OpBnnn(addr) => {
                self.index = addr as u16 + self.registers[0] as u16;
                self.program_counter += 1;
            },
            Instruction::OpCxkk(register_index, value) => {
                self.registers[register_index as usize] = self.get_random_u8() & value as u8;
                self.program_counter += 1;
            },
            Instruction::OpDxyn(x, y, value) => {
                let x_wrapped = x + 8 % DISPLAY_WIDTH;
                let y_wrapped = y + value % DISPLAY_HEIGHT;
                self.memory
                    .iter()
                    .skip(self.index as usize)
                    .take(value as usize)
                    // turn memory in iter of bools (bits)
                    .flat_map(|byte| 
                        (0..8)
                            .into_iter()
                            .map(move |index| (byte & (0b1000_0000 >> index)) > 0)
                    )
                    .zip(
                        // get section of vram we will draw at
                        self.vram
                            .iter_mut()
                            .enumerate()
                            .filter_map(|(index, pixel)| {
                                let px = index as u8 % DISPLAY_HEIGHT;
                                let py = index as u8 / DISPLAY_HEIGHT;
                                let x_is_valid = px < x_wrapped || (px >= x && px < x + 8);
                                let y_is_valid = py < y_wrapped || (py >= y && py < y + value);
                                if x_is_valid && y_is_valid {
                                    return Some(pixel);
                                }
                                None
                            })
                            
                    )
                    .for_each(|(memory_bit, vram_bit)| {
                        if *vram_bit == memory_bit && !*vram_bit {
                            self.registers[NUM_REGISTERS - 1] = 1;
                        }
                        *vram_bit = *vram_bit != memory_bit;
                    });
                self.program_counter += 1;
            },
            Instruction::OpEx9E(register_index) => {
                if keypad[self.registers[register_index as usize] as usize] {
                    self.program_counter += 2;
                }
                self.program_counter += 1;
            },
            Instruction::OpExA1(register_index) => {
                if !keypad[self.registers[register_index as usize] as usize] {
                    self.program_counter += 2;
                }
                self.program_counter += 1;
            },
            Instruction::OpFx07(register_index) => {
                self.registers[register_index as usize] = self.delay_timer;
                self.program_counter += 1;
            },
            Instruction::OpFx0A(register_index) => {
                self.waiting_for_key = Some(self.registers[register_index as usize] as usize);
                self.program_counter += 1;
            },
            Instruction::OpFx15(register_index) => {
                self.delay_timer = self.registers[register_index as usize];
                self.program_counter += 1;
            },
            Instruction::OpFx18(register_index) => {
                self.sound_timer = self.registers[register_index as usize];
                self.program_counter += 1;
            },
            Instruction::OpFx1E(register_index) => {
                self.index = self.index + self.registers[register_index as usize] as u16;
                self.program_counter += 1;
            },
            Instruction::OpFx29(register_index) => {
                let value : usize = (self.registers[register_index as usize] & 0x0F) as usize;
                self.index = (FONT_SET.len() + value * FONT_SIZE) as u16;
                self.program_counter += 1;
            },
            Instruction::OpFx33(register_index) => {
                let mut value = self.registers[register_index as usize];
                let mut digits : [u8; 3] = [0, 0, value % 10];
                value /= 10;
                digits[1] = value % 10;
                value /= 10;
                digits[0] = value;
                self.memory
                    .iter_mut()
                    .skip(self.index as usize)
                    .take(3)
                    .zip(digits)
                    .for_each(|(byte, digit)| {
                        *byte = digit;
                    });
                self.program_counter += 1;
            },
            Instruction::OpFx55(register_index) => {
                self.memory
                    .iter_mut()
                    .skip(self.index as usize)
                    .take(register_index as usize)
                    .zip(self.registers)
                    .for_each(|(byte, register)| {
                        *byte = register
                    });
                self.program_counter += 1;
            },
            Instruction::OpFx65(register_index) => {
                self.memory
                    .iter()
                    .skip(self.index as usize)
                    .take(register_index as usize)
                    .zip(self.registers.iter_mut())
                    .for_each(|(byte, register)| {
                        *register = *byte;
                    });
                self.program_counter += 1;
            },
        }
    }
}