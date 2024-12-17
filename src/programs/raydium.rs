use bs58;

pub struct RaydiumAddresses {
    pub program_id: &'static str,
}

pub struct RaydiumDiscriminators {
    pub initialize: u8,
    pub initialize2: u8,
}

pub struct Raydium {
    pub addresses: RaydiumAddresses,
    pub discriminators: RaydiumDiscriminators,
}

pub enum RaydiumFunction {
    Initialize,
    Initialize2
}

pub const RAYDIUM: Raydium = Raydium {
    addresses: RaydiumAddresses {
        program_id: "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8",
    },
    discriminators: RaydiumDiscriminators {
        initialize: 0x00,
        initialize2: 0x01,
    },
};

impl RaydiumFunction {
    pub fn from_data(data: &str) -> Option<RaydiumFunction> {
        let bytes = bs58::decode(data).into_vec().ok()?;

        if data.len() < 1 {
            return None;
        }

        let discriminator = bytes[0];
        match discriminator {
            x if x == RAYDIUM.discriminators.initialize => Some(RaydiumFunction::Initialize),
            x if x == RAYDIUM.discriminators.initialize2 => Some(RaydiumFunction::Initialize2),
            _ => None,
        }
    }
}