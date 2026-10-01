//! Physical dimensions, unit table, and dimensional conversions.

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Dim {
    pub length: i8,
    pub mass: i8,
    pub time: i8,
    pub data: i8,
    pub temp: i8,
}

impl Dim {
    pub const SCALAR: Dim = Dim { length: 0, mass: 0, time: 0, data: 0, temp: 0 };
    pub const LENGTH: Dim = Dim { length: 1, mass: 0, time: 0, data: 0, temp: 0 };
    pub const MASS: Dim = Dim { length: 0, mass: 1, time: 0, data: 0, temp: 0 };
    pub const TIME: Dim = Dim { length: 0, mass: 0, time: 1, data: 0, temp: 0 };
    pub const DATA: Dim = Dim { length: 0, mass: 0, time: 0, data: 1, temp: 0 };
    pub const TEMP: Dim = Dim { length: 0, mass: 0, time: 0, data: 0, temp: 1 };
    pub const AREA: Dim = Dim { length: 2, mass: 0, time: 0, data: 0, temp: 0 };
    pub const VOLUME: Dim = Dim { length: 3, mass: 0, time: 0, data: 0, temp: 0 };
    pub const SPEED: Dim = Dim { length: 1, mass: 0, time: -1, data: 0, temp: 0 };
    pub const FREQUENCY: Dim = Dim { length: 0, mass: 0, time: -1, data: 0, temp: 0 };

    pub fn product(self, other: Dim) -> Dim {
        Dim {
            length: self.length + other.length,
            mass: self.mass + other.mass,
            time: self.time + other.time,
            data: self.data + other.data,
            temp: self.temp + other.temp,
        }
    }

    pub fn quotient(self, other: Dim) -> Dim {
        Dim {
            length: self.length - other.length,
            mass: self.mass - other.mass,
            time: self.time - other.time,
            data: self.data - other.data,
            temp: self.temp - other.temp,
        }
    }

    pub fn is_scalar(self) -> bool {
        self == Self::SCALAR
    }

    pub fn name(self) -> &'static str {
        if self == Self::LENGTH {
            "length"
        } else if self == Self::MASS {
            "mass"
        } else if self == Self::TIME {
            "time"
        } else if self == Self::DATA {
            "data"
        } else if self == Self::TEMP {
            "temperature"
        } else if self.length == 3 && self.mass == 0 && self.time == 0 {
            "volume"
        } else if self.length == 2 && self.mass == 0 && self.time == 0 {
            "area"
        } else if self.length == 1 && self.time == -1 {
            "speed"
        } else if self == Self::FREQUENCY {
            "frequency"
        } else {
            "dimension"
        }
    }
}

pub struct UnitDef {
    pub symbol: &'static str,
    pub aliases: &'static [&'static str],
    pub dim: Dim,
    pub factor: f64,
}

pub const UNITS: &[UnitDef] = &[
    // Length (base: meter)
    UnitDef { symbol: "m", aliases: &["m", "meter", "meters", "metre", "metres"], dim: Dim::LENGTH, factor: 1.0 },
    UnitDef { symbol: "km", aliases: &["km", "kilometer", "kilometers", "kilometre", "kilometres"], dim: Dim::LENGTH, factor: 1000.0 },
    UnitDef { symbol: "cm", aliases: &["cm", "centimeter", "centimeters", "centimetre", "centimetres"], dim: Dim::LENGTH, factor: 0.01 },
    UnitDef { symbol: "mm", aliases: &["mm", "millimeter", "millimeters", "millimetre", "millimetres"], dim: Dim::LENGTH, factor: 0.001 },
    UnitDef { symbol: "mi", aliases: &["mi", "mile", "miles"], dim: Dim::LENGTH, factor: 1609.344 },
    UnitDef { symbol: "yd", aliases: &["yd", "yard", "yards"], dim: Dim::LENGTH, factor: 0.9144 },
    UnitDef { symbol: "ft", aliases: &["ft", "foot", "feet"], dim: Dim::LENGTH, factor: 0.3048 },
    UnitDef { symbol: "in", aliases: &["in", "inch", "inches"], dim: Dim::LENGTH, factor: 0.0254 },

    // Mass (base: kilogram)
    UnitDef { symbol: "kg", aliases: &["kg", "kilogram", "kilograms", "kilo", "kilos"], dim: Dim::MASS, factor: 1.0 },
    UnitDef { symbol: "g", aliases: &["g", "gram", "grams"], dim: Dim::MASS, factor: 0.001 },
    UnitDef { symbol: "mg", aliases: &["mg", "milligram", "milligrams"], dim: Dim::MASS, factor: 0.000001 },
    UnitDef { symbol: "t", aliases: &["t", "ton", "tons", "tonne", "tonnes"], dim: Dim::MASS, factor: 1000.0 },
    UnitDef { symbol: "lb", aliases: &["lb", "lbs", "pound", "pounds"], dim: Dim::MASS, factor: 0.453_592_37 },
    UnitDef { symbol: "oz", aliases: &["oz", "ounce", "ounces"], dim: Dim::MASS, factor: 0.028_349_523_125 },

    // Time (base: second)
    UnitDef { symbol: "s", aliases: &["s", "sec", "secs", "second", "seconds"], dim: Dim::TIME, factor: 1.0 },
    UnitDef { symbol: "ms", aliases: &["ms", "millisecond", "milliseconds"], dim: Dim::TIME, factor: 0.001 },
    UnitDef { symbol: "min", aliases: &["min", "minute", "minutes"], dim: Dim::TIME, factor: 60.0 },
    UnitDef { symbol: "h", aliases: &["h", "hr", "hrs", "hour", "hours"], dim: Dim::TIME, factor: 3600.0 },
    UnitDef { symbol: "d", aliases: &["d", "day", "days"], dim: Dim::TIME, factor: 86400.0 },
    UnitDef { symbol: "wk", aliases: &["wk", "week", "weeks"], dim: Dim::TIME, factor: 604_800.0 },
    UnitDef { symbol: "yr", aliases: &["yr", "year", "years"], dim: Dim::TIME, factor: 31_536_000.0 },

    // Data (base: byte)
    UnitDef { symbol: "b", aliases: &["b", "byte", "bytes"], dim: Dim::DATA, factor: 1.0 },
    UnitDef { symbol: "kb", aliases: &["kb", "kilobyte", "kilobytes"], dim: Dim::DATA, factor: 1000.0 },
    UnitDef { symbol: "mb", aliases: &["mb", "megabyte", "megabytes"], dim: Dim::DATA, factor: 1_000_000.0 },
    UnitDef { symbol: "gb", aliases: &["gb", "gigabyte", "gigabytes"], dim: Dim::DATA, factor: 1_000_000_000.0 },
    UnitDef { symbol: "tb", aliases: &["tb", "terabyte", "terabytes"], dim: Dim::DATA, factor: 1_000_000_000_000.0 },
    UnitDef { symbol: "kib", aliases: &["kib", "kibibyte", "kibibytes"], dim: Dim::DATA, factor: 1024.0 },
    UnitDef { symbol: "mib", aliases: &["mib", "mebibyte", "mebibytes"], dim: Dim::DATA, factor: 1_048_576.0 },
    UnitDef { symbol: "gib", aliases: &["gib", "gibibyte", "gibibytes"], dim: Dim::DATA, factor: 1_073_741_824.0 },

    // Volume (base: cubic meter m^3)
    UnitDef { symbol: "l", aliases: &["l", "liter", "liters", "litre", "litres"], dim: Dim::VOLUME, factor: 0.001 },
    UnitDef { symbol: "ml", aliases: &["ml", "milliliter", "milliliters", "millilitre", "millilitres"], dim: Dim::VOLUME, factor: 0.000001 },
    UnitDef { symbol: "gal", aliases: &["gal", "gallon", "gallons"], dim: Dim::VOLUME, factor: 0.003785411784 },
    UnitDef { symbol: "qt", aliases: &["qt", "quart", "quarts"], dim: Dim::VOLUME, factor: 0.000946352946 },
    UnitDef { symbol: "pt", aliases: &["pt", "pint", "pints"], dim: Dim::VOLUME, factor: 0.000473176473 },
    UnitDef { symbol: "cup", aliases: &["cup", "cups"], dim: Dim::VOLUME, factor: 0.0002365882365 },
    UnitDef { symbol: "floz", aliases: &["floz", "fl oz"], dim: Dim::VOLUME, factor: 0.0000295735295625 },
    UnitDef { symbol: "m³", aliases: &["m3", "m^3", "cubic meter", "cubic meters"], dim: Dim::VOLUME, factor: 1.0 },

    // Area (base: square meter m^2)
    UnitDef { symbol: "m²", aliases: &["m2", "m^2", "sqm", "sq meter"], dim: Dim::AREA, factor: 1.0 },
    UnitDef { symbol: "km²", aliases: &["km2", "km^2", "sqkm"], dim: Dim::AREA, factor: 1_000_000.0 },
    UnitDef { symbol: "sqft", aliases: &["sqft", "sq ft", "ft2", "ft^2"], dim: Dim::AREA, factor: 0.09290304 },
    UnitDef { symbol: "acre", aliases: &["acre", "acres"], dim: Dim::AREA, factor: 4046.8564224 },
    UnitDef { symbol: "ha", aliases: &["ha", "hectare", "hectares"], dim: Dim::AREA, factor: 10000.0 },

    // Speed (base: m/s)
    UnitDef { symbol: "m/s", aliases: &["m/s", "mps"], dim: Dim::SPEED, factor: 1.0 },
    UnitDef { symbol: "km/h", aliases: &["km/h", "kph", "kmh"], dim: Dim::SPEED, factor: 1.0 / 3.6 },
    UnitDef { symbol: "mph", aliases: &["mph"], dim: Dim::SPEED, factor: 0.44704 },
    UnitDef { symbol: "knot", aliases: &["knot", "knots", "kt"], dim: Dim::SPEED, factor: 0.514444 },

    // Frequency (base: 1/s = Hz)
    UnitDef { symbol: "hz", aliases: &["hz", "hertz"], dim: Dim::FREQUENCY, factor: 1.0 },
    UnitDef { symbol: "khz", aliases: &["khz", "kilohertz"], dim: Dim::FREQUENCY, factor: 1000.0 },
    UnitDef { symbol: "mhz", aliases: &["mhz", "megahertz"], dim: Dim::FREQUENCY, factor: 1_000_000.0 },
    UnitDef { symbol: "ghz", aliases: &["ghz", "gigahertz"], dim: Dim::FREQUENCY, factor: 1_000_000_000.0 },
];

pub fn find_unit(name: &str) -> Option<&'static UnitDef> {
    let lower = name.trim().to_lowercase();
    UNITS.iter().find(|u| u.aliases.contains(&lower.as_str()))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Temperature {
    Celsius,
    Fahrenheit,
    Kelvin,
}

impl Temperature {
    pub fn label(self) -> &'static str {
        match self {
            Self::Celsius => "°C",
            Self::Fahrenheit => "°F",
            Self::Kelvin => "K",
        }
    }
}

pub fn find_temperature(name: &str) -> Option<Temperature> {
    match name.trim().to_lowercase().as_str() {
        "c" | "celsius" | "centigrade" | "°c" => Some(Temperature::Celsius),
        "f" | "fahrenheit" | "°f" => Some(Temperature::Fahrenheit),
        "k" | "kelvin" => Some(Temperature::Kelvin),
        _ => None,
    }
}

pub fn convert_temperature(value: f64, from: Temperature, to: Temperature) -> f64 {
    let celsius = match from {
        Temperature::Celsius => value,
        Temperature::Fahrenheit => (value - 32.0) * 5.0 / 9.0,
        Temperature::Kelvin => value - 273.15,
    };
    match to {
        Temperature::Celsius => celsius,
        Temperature::Fahrenheit => (celsius * 9.0 / 5.0) + 32.0,
        Temperature::Kelvin => celsius + 273.15,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UnitMismatchError {
    pub from_dim: Dim,
    pub to_dim: Dim,
}

pub fn convert_units(
    value: f64,
    from: &str,
    to: &str,
) -> Result<(f64, String), Result<UnitMismatchError, ()>> {
    if let (Some(from_t), Some(to_t)) = (find_temperature(from), find_temperature(to)) {
        return Ok((convert_temperature(value, from_t, to_t), to_t.label().to_string()));
    }

    let from_unit = find_unit(from).ok_or(Err(()))?;
    let to_unit = find_unit(to).ok_or(Err(()))?;

    if from_unit.dim != to_unit.dim {
        return Err(Ok(UnitMismatchError {
            from_dim: from_unit.dim,
            to_dim: to_unit.dim,
        }));
    }

    let converted = value * from_unit.factor / to_unit.factor;
    Ok((converted, to_unit.symbol.to_string()))
}

/// Provides natural default conversion for single-quantity input (e.g. 1m -> 3.28 ft).
pub fn default_conversion(value: f64, from: &str) -> Option<(f64, String)> {
    if let Some(temp) = find_temperature(from) {
        let target = match temp {
            Temperature::Celsius => Temperature::Fahrenheit,
            Temperature::Fahrenheit => Temperature::Celsius,
            Temperature::Kelvin => Temperature::Celsius,
        };
        return Some((convert_temperature(value, temp, target), target.label().to_string()));
    }

    let unit = find_unit(from)?;
    let target_symbol = match unit.symbol {
        "m" => "ft",
        "km" => "mi",
        "cm" | "mm" => "in",
        "mi" => "km",
        "ft" | "yd" => "m",
        "in" => "cm",
        "kg" => "lb",
        "g" | "mg" => "oz",
        "lb" => "kg",
        "oz" => "g",
        "l" => "gal",
        "ml" => "floz",
        "gal" => "l",
        "h" => "min",
        "min" => "s",
        "d" => "h",
        "gb" => "mb",
        "mb" => "gb",
        _ => return None,
    };

    convert_units(value, from, target_symbol).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_length_and_mass() {
        let (val, sym) = convert_units(10.0, "kg", "lb").unwrap();
        assert!((val - 22.0462).abs() < 0.01);
        assert_eq!(sym, "lb");

        let (val, sym) = convert_units(10.0, "km", "mi").unwrap();
        assert!((val - 6.2137).abs() < 0.01);
        assert_eq!(sym, "mi");
    }

    #[test]
    fn detects_dimensional_mismatch() {
        let err = convert_units(10.0, "kg", "gal").unwrap_err().unwrap();
        assert_eq!(err.from_dim.name(), "mass");
        assert_eq!(err.to_dim.name(), "volume");
    }
}
