use bs58;

pub struct SerumAddresses {
    pub program_id: &'static str,
}

pub struct SerumDiscriminators {
    pub initialize_market: u8,
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
        initialize_market: 0x00,
    },
};

impl SerumFunction {
    pub fn from_data(data: &str) -> Option<SerumFunction> {
        let bytes = bs58::decode(data).into_vec().ok()?;

        if data.len() < 1 {
            return None;
        }

        let discriminator = bytes[0];
        match discriminator {
            x if x == SERUM.discriminators.initialize_market => Some(SerumFunction::InitializeMarket),
            _ => None,
        }
    }
}