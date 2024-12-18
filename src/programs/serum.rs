/// Some Form of IDL: https://github.com/project-serum/serum-dex/blob/master/dex/src/instruction.rs

use bs58;

pub struct SerumAddresses {
    pub program_id: &'static str,
}

pub struct SerumDiscriminators {
    pub initialize_market: u32,
}

pub struct Serum {
    pub addresses: SerumAddresses,
    pub discriminators: SerumDiscriminators,
}

pub enum SerumFunction {
    InitializeMarket,
}

pub const SERUM: Serum = Serum {
    addresses: SerumAddresses {
        program_id: "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
    },
    discriminators: SerumDiscriminators {
        initialize_market: 0x00000000,
    },
};

impl SerumFunction {
    pub fn from_data(data: &str) -> Option<SerumFunction> {
        let bytes = bs58::decode(data).into_vec().ok()?;

        if data.len() < 5 {
            return None;
        }

        let _version = bytes[0];

        let discriminator_bytes: [u8; 4] = bytes[1..5].try_into().ok()?;
        let discriminator = u32::from_le_bytes(discriminator_bytes);

        match discriminator {
            x if x == SERUM.discriminators.initialize_market => Some(SerumFunction::InitializeMarket),
            _ => None,
        }
    }
}