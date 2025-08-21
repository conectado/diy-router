use macros::make_enum;
use thiserror::Error;

#[derive(Default)]
pub struct Enc28j60<const N: usize = 50, const M: usize = 10> {
    current_bank: Bank,
    pending_transactions: Transactions<N, M>,
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

    pub fn new() -> Self {
        Default::default()
    }

    pub fn poll_pending_transaction(
        &mut self,
    ) -> Option<heapless::Deque<ControlRegisterOperation, N>> {
        self.pending_transactions.pop_transaction()
    }

    // TODO: internally buffer operations?
    /// Requires at least 2 positions for operations.
    pub fn read_register(&mut self, register: ControlRegister) -> Result<(), TransactionError> {
        if register.bank != self.current_bank {
            self.pending_transactions.new_transaction()?;
            // TODO: Bit flags? bit set?
            self.pending_transactions
                .push_operation(ControlRegisterOperation::Write(heapless::Vec::from_iter(
                    [OpCode::WCR as u8 | Self::ECON as u8, register.bank as u8].into_iter(),
                )))?;

            self.current_bank = register.bank;
        }

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
