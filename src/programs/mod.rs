pub mod pump_fun;

pub enum ProgramId {
    PumpFun
}

impl ProgramId {
    pub fn from_str(program_id: &str) -> Option<ProgramId> {
        match program_id {
            x if x == pump_fun::PUMP_FUN.addresses.program_id => Some(ProgramId::PumpFun),
            _ => None,
        }
    }
}