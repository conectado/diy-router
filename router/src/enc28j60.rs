use core::ops::RangeInclusive;

use macros::make_enum;
use thiserror::Error;

pub struct Enc28j60<const N: usize = 50, const M: usize = 10> {
    current_bank: Bank,
    pending_transactions: Transactions<N, M>,
    erx_range: RangeInclusive<ux::u9>,
    ready: bool,
}

//// One of 4 memory banks for control registers.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bank {
    Bank0 = 0b00,
    Bank1 = 0b01,
    Bank2 = 0b10,
    Bank3 = 0b11,
}

impl Default for Bank {
    fn default() -> Self {
        Bank::Bank0
    }
}

make_enum!(pub RegisterAddress, 5);

/// Represents a single control register
#[derive(Debug, Clone, Copy)]
pub struct ControlRegister {
    pub bank: Bank,
    pub address: RegisterAddress,
}

impl ControlRegister {
    fn next(&self) -> ControlRegister {
        ControlRegister {
            address: self.address.next(),
            ..*self
        }
    }
}

/// Operation Code for interfacing with ENC28j60.
// TODO: is there a way in the type system to represent that some of these are 3-bits + 5-bit address vs other that are just 8 bits?
#[repr(u8)]
enum OpCode {
    /// Read control register.
    RCR = 0b000_00000,
    /// Read buffer memory.
    RBM = 0b001_11010,
    /// Write control register.
    WCR = 0b010_00000,
    /// Write buffer memory.
    WBM = 0b011_11010,
    /// Bit field set.
    BFS = 0b100_00000,
    /// Bit field clear.
    BFC = 0b101_00000,
    /// System reset command.
    SRC = 0b111_11111,
}

#[derive(Default)]
struct Transactions<const N: usize, const M: usize> {
    buffer: heapless::Deque<ControlRegisterOperation, N>,
    bounds: heapless::Deque<usize, M>,
}

#[derive(Error, Debug)]
pub enum TransactionError {
    #[error("Buffer ran out of memory for additional operations.")]
    OperationsOutOfMemory,
    #[error("Buffer ran out of memory for additional transactions.")]
    TransactionOutOfMemory,
}

impl<'a, const N: usize, const M: usize> Transactions<N, M> {
    fn push_operation(
        &mut self,
        operation: ControlRegisterOperation,
    ) -> Result<(), TransactionError> {
        self.buffer
            .push_back(operation)
            .map_err(|_| TransactionError::OperationsOutOfMemory)?;

        if self.bounds.is_empty() {
            self.bounds.push_back(0).unwrap();
        }

        let bound = self.bounds.back_mut().unwrap();
        *bound += 1;

        Ok(())
    }

    fn new_transaction(&mut self) -> Result<(), TransactionError> {
        self.bounds
            .push_back(0)
            .map_err(|_| TransactionError::TransactionOutOfMemory)?;
        Ok(())
    }

    fn pop_transaction(&mut self) -> Option<heapless::Deque<ControlRegisterOperation, N>> {
        let boundary = self.bounds.pop_front()?;
        let mut result = heapless::Deque::new();
        for _ in 0..boundary {
            result.push_back(self.buffer.pop_front().unwrap()).unwrap();
        }

        Some(result)
    }
}

impl<const N: usize, const M: usize> Enc28j60<N, M> {
    const ECON: RegisterAddress = RegisterAddress::r1F;
    const ESTAT: RegisterAddress = RegisterAddress::r1D;

    // TODO: better represent that these are words
    const ERXSTL: ControlRegister = ControlRegister {
        bank: Bank::Bank0,
        address: RegisterAddress::r08,
    };
    const ERXNDL: ControlRegister = ControlRegister {
        bank: Bank::Bank0,
        address: RegisterAddress::r0A,
    };
    const ERXDPTL: ControlRegister = ControlRegister {
        bank: Bank::Bank0,
        address: RegisterAddress::r0C,
    };

    const ERXFCON: ControlRegister = ControlRegister {
        bank: Bank::Bank1,
        address: RegisterAddress::r18,
    };

    const MACON1: ControlRegister = ControlRegister {
        bank: Bank::Bank2,
        address: RegisterAddress::r00,
    };

    const MACON3: ControlRegister = ControlRegister {
        bank: Bank::Bank2,
        address: RegisterAddress::r02,
    };
    const MACON4: ControlRegister = ControlRegister {
        bank: Bank::Bank2,
        address: RegisterAddress::r03,
    };

    pub fn with_erx_range(erx_range: RangeInclusive<ux::u9>) -> Self {
        Self {
            current_bank: Default::default(),
            pending_transactions: Default::default(),
            erx_range,
            ready: false,
        }
    }

    pub fn with_erx_length(length: ux::u9) -> Self {
        Self {
            current_bank: Default::default(),
            pending_transactions: Default::default(),
            erx_range: (ux::u9::min_value())..=length,
            ready: false,
        }
    }

    pub fn init(&mut self) -> Result<(), TransactionError> {
        let start = (*self.erx_range.start()).into();
        let end = (*self.erx_range.end()).into();

        // Initialize receive buffer
        // NOTE: Waiting for osc is baked in poll_pending.
        // it could be done after ETH config, which would be ideal
        // but it's kept there for simplicity right now.
        self.write_word(Self::ERXSTL, start)?;
        self.write_word(Self::ERXNDL, end)?;
        self.write_word(Self::ERXDPTL, start)?;

        // Initialize Receieve filters
        // TODO: for now we go promiscuous 😏
        self.write_register(Self::ERXFCON, 0x00)?;

        // Initialize MAC
        // TODO: expose config
        self.write_register(Self::MACON1, 0b0000_1101)?;
        self.write_word(Self::MACON3, 0b111_1_0_1_1_1)?;
        self.write_word(Self::MACON4, 0b0_0_0_0_0_0)?;

        // TODO: Phy initialize?

        Ok(())
    }

    pub fn poll_pending_transaction(
        &mut self,
    ) -> Option<heapless::Deque<ControlRegisterOperation, N>> {
        if !self.ready {
            let mut result = heapless::Deque::new();

            let mut read_buffer = heapless::Vec::new();
            read_buffer.push(0).unwrap();

            result
                .push_back(ControlRegisterOperation::Write(heapless::Vec::from_iter(
                    [OpCode::RCR as u8 | Self::ESTAT as u8].into_iter(),
                )))
                .unwrap();
            result
                .push_back(ControlRegisterOperation::Read(read_buffer))
                .unwrap();

            return Some(result);
        }

        self.pending_transactions.pop_transaction()
    }

    fn write_to_control_register_address(
        &mut self,
        address: RegisterAddress,
        value: u8,
    ) -> Result<(), TransactionError> {
        self.pending_transactions.new_transaction()?;
        self.pending_transactions
            .push_operation(ControlRegisterOperation::Write(heapless::Vec::from_iter(
                [OpCode::WCR as u8 | address as u8, value].into_iter(),
            )))?;
        Ok(())
    }

    fn bit_field_set_to_control_register_address(
        &mut self,
        address: RegisterAddress,
        value: u8,
    ) -> Result<(), TransactionError> {
        self.pending_transactions.new_transaction()?;
        self.pending_transactions
            .push_operation(ControlRegisterOperation::Write(heapless::Vec::from_iter(
                [OpCode::BFS as u8 | address as u8, value].into_iter(),
            )))?;
        Ok(())
    }

    fn set_bank(&mut self, bank: Bank) -> Result<(), TransactionError> {
        if bank == self.current_bank {
            return Ok(());
        }

        self.bit_field_set_to_control_register_address(Self::ECON, bank as u8)?;

        self.current_bank = bank;
        Ok(())
    }

    fn write_word(
        &mut self,
        register: ControlRegister,
        value: u16,
    ) -> Result<(), TransactionError> {
        let [low, high] = value.to_be_bytes();
        self.write_register(register, low)?;
        self.write_register(register.next(), high)?;
        Ok(())
    }

    pub fn handle_transaction(
        &mut self,
        // TODO: feeding operations like this is awful as we need to match over the transactions
        // what we ideally would want is to keep some struct with all the details of the original operations with references to buffers
        // this function here shows also how we could actually update buffers here and never copy operations around.
        mut transaction: heapless::Deque<ControlRegisterOperation, N>,
    ) {
        match transaction.pop_front() {
            Some(ControlRegisterOperation::Write(b)) => {
                if b.contains(&(OpCode::RCR as u8 | Self::ESTAT as u8)) {
                    let Some(ControlRegisterOperation::Read(operation)) = transaction.pop_front()
                    else {
                        // TODO: with a good operation wrapper we wouldn't need to panic here.
                        panic!("Inconsistent transaction: reading ESTAT without a read buffer");
                    };

                    if operation[0] & 0b0000_0001 == 1 {
                        self.ready = true;
                    }
                }
            }
            Some(_) => {}
            None => {
                return;
            }
        }
    }

    fn write_register(
        &mut self,
        register: ControlRegister,
        value: u8,
    ) -> Result<(), TransactionError> {
        self.set_bank(register.bank)?;
        self.write_to_control_register_address(register.address, value)
    }

    // TODO: internally buffer operations?
    /// Requires at least 2 positions for operations.
    pub fn read_register(&mut self, register: ControlRegister) -> Result<(), TransactionError> {
        self.set_bank(register.bank)?;

        self.pending_transactions.new_transaction()?;
        self.pending_transactions
            .push_operation(ControlRegisterOperation::Write(heapless::Vec::from_iter(
                [OpCode::RCR as u8 | register.address as u8].into_iter(),
            )))?;
        // TODO: oh no no no
        let mut read_buffer = heapless::Vec::new();
        read_buffer.push(0).unwrap();
        self.pending_transactions
            .push_operation(ControlRegisterOperation::Read(read_buffer))?;
        Ok(())
    }
}

/// Control register operations are treated separatedly to own the buffers.
/// TODO: I don't really want to think right now how to deal with the write/read memory buffer operations yet but they might be simpler,
/// as they might need single packets
/// DMA is a whole other beast.
/// This is just to continue prototyping
#[derive(Debug, PartialEq, Eq)]
pub enum ControlRegisterOperation {
    Read(heapless::Vec<u8, 2>),
    Write(heapless::Vec<u8, 2>),
}

impl<'a> From<&'a mut ControlRegisterOperation> for embedded_hal::spi::Operation<'a, u8> {
    fn from(value: &'a mut ControlRegisterOperation) -> Self {
        match value {
            ControlRegisterOperation::Read(buffer) => {
                embedded_hal::spi::Operation::Read(buffer.as_mut_slice())
            }
            ControlRegisterOperation::Write(buffer) => {
                embedded_hal::spi::Operation::Write(buffer.as_slice())
            }
        }
    }
}
