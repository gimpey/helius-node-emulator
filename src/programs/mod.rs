pub mod pump_fun;
pub mod raydium;
pub mod serum;

pub enum ProgramId {
    PumpFun,
    Raydium,
    Serum
}

impl ProgramId {
    pub fn from_str(program_id: &str) -> Option<ProgramId> {
        match program_id {
            x if x == pump_fun::PUMP_FUN.addresses.program_id => Some(ProgramId::PumpFun),
            x if x == raydium::RAYDIUM.addresses.program_id => Some(ProgramId::Raydium),
            x if x == serum::SERUM.addresses.program_id => Some(ProgramId::Serum),
            _ => None,
        }
    }
}