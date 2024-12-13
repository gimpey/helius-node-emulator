use bs58;

pub struct PumpFunAddresses {
    pub program_id: &'static str,
}

pub struct PumpFunDiscriminators {
    pub creation: u64,
    pub buy: u64,
    pub sell: u64,
}

pub struct PumpFun {
    pub addresses: PumpFunAddresses,
    pub discriminators: PumpFunDiscriminators,
}

pub enum PumpFunFunction {
    Creation,
    Buy,
    Sell,
}

pub const PUMP_FUN: PumpFun = PumpFun {
    addresses: PumpFunAddresses {
        program_id: "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P",
    },
    discriminators: PumpFunDiscriminators {
        creation: 0x181ec828051c0777,
        buy: 0x66063d1201daebea,
        sell: 0x33e685a4017f83ad,
    },
};

impl PumpFunFunction {
    pub fn from_data(data: &str) -> Option<PumpFunFunction> {
        let bytes = bs58::decode(data).into_vec().ok()?;

        if data.len() < 8 {
            return None;
        }

        let discriminator = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        match discriminator {
            x if x == PUMP_FUN.discriminators.creation => Some(PumpFunFunction::Creation),
            x if x == PUMP_FUN.discriminators.buy => Some(PumpFunFunction::Buy),
            x if x == PUMP_FUN.discriminators.sell => Some(PumpFunFunction::Sell),
            _ => None,
        }
    }
}