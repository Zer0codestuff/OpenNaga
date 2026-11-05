//! Razer 90-byte control reports. MIT implementation from wire-format facts,
//! not translated OpenRazer code. References, accessed 2026-09-08:
//! https://github.com/openrazer/openrazer/blob/master/driver/razerchromacommon.c
//! https://github.com/openrazer/openrazer/blob/master/driver/razermouse_driver.c
//! https://github.com/openrazer/openrazer/blob/master/driver/razercommon.c
//! https://github.com/openrazer/openrazer/pull/2850
//! Scoped to the Naga V2 HyperSpeed receiver 1532:00b4, not Bluetooth.

use std::fmt;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RazerError {
    InvalidValue(String),
    Malformed(String),
    Status(u8),
    Disconnected,
    Transport(String),
    Readback,
}

impl fmt::Display for RazerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            RazerError::InvalidValue(t) | RazerError::Malformed(t) | RazerError::Transport(t) => t.clone(),
            RazerError::Disconnected => "Naga V2 HyperSpeed USB receiver unavailable.".into(),
            RazerError::Status(1) => "Mouse busy. Try again.".into(),
            RazerError::Status(3) => "The mouse rejected the command.".into(),
            RazerError::Status(4) => "Mouse timeout. Move it to wake it up.".into(),
            RazerError::Status(5) => "Command not supported by the mouse.".into(),
            RazerError::Status(value) => format!("Unknown response status: {value}."),
            RazerError::Readback => "The value read back does not confirm the requested change.".into(),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for RazerError {}

pub type Result<T> = std::result::Result<T, RazerError>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RazerCommand {
    pub transaction: u8,
    pub class: u8,
    pub id: u8,
    pub arguments: Vec<u8>,
}

impl RazerCommand {
    pub fn new(transaction: u8, class: u8, id: u8, arguments: Vec<u8>) -> Self {
        RazerCommand { transaction, class, id, arguments }
    }
    pub fn get_dpi() -> Self {
        Self::new(0x1f, 4, 0x85, vec![0; 7])
    }
    pub fn get_polling() -> Self {
        Self::new(0x1f, 0, 0x85, vec![0])
    }
    pub fn get_battery() -> Self {
        Self::new(0x1f, 7, 0x80, vec![0, 0])
    }
    /// OpenRazer reads the mode with 0xff on this receiver, unlike SET mode.
    /// The Naga V2 HyperSpeed answers status 4 to that variant, so the
    /// session retries with 0x1f, the ID every other command uses here.
    pub fn get_mode() -> Self {
        Self::new(0xff, 0, 0x84, vec![0, 0])
    }
    pub fn get_mode_alternate() -> Self {
        Self::new(0x1f, 0, 0x84, vec![0, 0])
    }
    pub fn set_dpi(x: i32, y: i32) -> Result<Self> {
        if !(100..=30000).contains(&x) || !(100..=30000).contains(&y) {
            return Err(RazerError::InvalidValue("Allowed DPI: 100...30000.".into()));
        }
        // The published SET builder uses storage=1 even when passed NOSTORE.
        Ok(Self::new(0x1f, 4, 5, vec![1, (x >> 8) as u8, (x & 255) as u8, (y >> 8) as u8, (y & 255) as u8, 0, 0]))
    }
    pub fn set_polling(hz: i32) -> Result<Self> {
        let value = match hz {
            125 => 8,
            500 => 2,
            1000 => 1,
            _ => return Err(RazerError::InvalidValue("Allowed polling rates: 125, 500 or 1000 Hz.".into())),
        };
        Ok(Self::new(0x1f, 0, 5, vec![value]))
    }
    pub fn set_mode(mode: u8) -> Result<Self> {
        if mode != 0 && mode != 3 {
            return Err(RazerError::InvalidValue("Unsafe hardware mode.".into()));
        }
        Ok(Self::new(0x1f, 0, 4, vec![mode, 0]))
    }
}

pub const REPORT_LENGTH: usize = 90;

pub fn checksum(bytes: &[u8]) -> u8 {
    bytes[2..88].iter().fold(0, |a, b| a ^ b)
}

pub fn encode(command: &RazerCommand) -> Result<Vec<u8>> {
    if command.arguments.len() > 80 {
        return Err(RazerError::InvalidValue("Command too long.".into()));
    }
    let mut bytes = vec![0u8; REPORT_LENGTH];
    bytes[1] = command.transaction;
    bytes[5] = command.arguments.len() as u8;
    bytes[6] = command.class;
    bytes[7] = command.id;
    bytes[8..8 + command.arguments.len()].copy_from_slice(&command.arguments);
    bytes[88] = checksum(&bytes);
    Ok(bytes)
}

pub fn decode(bytes: &[u8], command: &RazerCommand) -> Result<Vec<u8>> {
    if bytes.len() != REPORT_LENGTH {
        return Err(RazerError::Malformed(format!("USB response: expected 90 bytes, received {}.", bytes.len())));
    }
    if bytes[88] != checksum(bytes) {
        return Err(RazerError::Malformed("Invalid response checksum.".into()));
    }
    if bytes[1] != command.transaction
        || bytes[6] != command.class
        || bytes[7] != command.id
        || bytes[2] != 0
        || bytes[3] != 0
        || bytes[4] != 0
        || bytes[89] != 0
    {
        return Err(RazerError::Malformed("The response does not match the command.".into()));
    }
    if bytes[5] > 80 {
        return Err(RazerError::Malformed("Invalid argument size.".into()));
    }
    if bytes[0] != 2 {
        return Err(RazerError::Status(bytes[0]));
    }
    if bytes[5] as usize != command.arguments.len() {
        return Err(RazerError::Malformed("Incomplete response.".into()));
    }
    Ok(bytes[8..8 + bytes[5] as usize].to_vec())
}

/// A receiver can expose a busy or previous command's valid reply while a
/// flash operation finishes. Poll GET_REPORT again without repeating SET.
pub fn needs_response_poll(bytes: &[u8], request: &[u8]) -> bool {
    if bytes.len() != REPORT_LENGTH
        || request.len() != REPORT_LENGTH
        || bytes[88] != checksum(bytes)
        || bytes[5] > 80
        || !(1..=5).contains(&bytes[0])
        || [2usize, 3, 4, 89].iter().any(|i| bytes[*i] != 0)
    {
        return false;
    }
    if bytes[1] != request[1] || bytes[6] != request[6] || bytes[7] != request[7] {
        return true;
    }
    if bytes[0] == 1 {
        return true;
    }
    // Consecutive onboard reads share the command header. Their bank and
    // control ID must match too; byte 2 of the assignment is reply metadata.
    bytes[0] == 2 && request[6] == 2 && request[7] == 0x8c && bytes[5] == 10 && (bytes[8] != request[8] || bytes[9] != request[9])
}

pub fn finish_response(
    initial: Vec<u8>,
    request: &[u8],
    pause: &mut dyn FnMut(Duration),
    read: &mut dyn FnMut() -> Result<Vec<u8>>,
) -> Result<Vec<u8>> {
    let mut response = initial;
    let deadline = Instant::now() + Duration::from_millis(500);
    for _ in 0..20 {
        if !needs_response_poll(&response, request) {
            return Ok(response);
        }
        if Instant::now() >= deadline {
            break;
        }
        pause(Duration::from_millis(25));
        response = read()?;
    }
    if needs_response_poll(&response, request) {
        return Err(RazerError::Transport(
            "The mouse response did not finish. No command was resent. Wake the mouse and try again.".into(),
        ));
    }
    Ok(response)
}

/// A session owns its transport. Use from one worker thread, never the UI.
pub trait RazerTransport {
    fn identity(&self) -> String;
    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HardwareSnapshot {
    pub dpi_x: Option<i32>,
    pub dpi_y: Option<i32>,
    pub polling_rate: Option<i32>,
    pub battery_level: Option<i32>,
    pub mode: Option<u8>,
    pub warnings: Vec<String>,
}

pub struct Session<'a> {
    pub transport: &'a mut dyn RazerTransport,
    pause: fn(Duration),
}

impl<'a> Session<'a> {
    pub fn new(transport: &'a mut dyn RazerTransport) -> Self {
        Session { transport, pause: std::thread::sleep }
    }

    #[cfg(test)]
    pub fn without_pauses(transport: &'a mut dyn RazerTransport) -> Self {
        Session { transport, pause: |_| {} }
    }

    pub fn execute(&mut self, command: &RazerCommand) -> Result<Vec<u8>> {
        let request = encode(command)?;
        for attempt in 0..3 {
            match self.transport.exchange(&request).and_then(|r| decode(&r, command)) {
                Err(RazerError::Status(1)) if attempt < 2 => (self.pause)(Duration::from_millis(20 * (attempt + 1))),
                other => return other,
            }
        }
        Err(RazerError::Status(1))
    }

    pub fn read_dpi(&mut self) -> Result<(i32, i32)> {
        let a = self.execute(&RazerCommand::get_dpi())?;
        let x = a[1] as i32 * 256 + a[2] as i32;
        let y = a[3] as i32 * 256 + a[4] as i32;
        if !(100..=30000).contains(&x) || !(100..=30000).contains(&y) {
            return Err(RazerError::Malformed("The mouse returned invalid DPI values.".into()));
        }
        Ok((x, y))
    }

    pub fn read_polling(&mut self) -> Result<i32> {
        let a = self.execute(&RazerCommand::get_polling())?;
        match a[0] {
            1 => Ok(1000),
            2 => Ok(500),
            8 => Ok(125),
            _ => Err(RazerError::Malformed("Unknown hardware polling rate.".into())),
        }
    }

    pub fn read_mode(&mut self) -> Result<u8> {
        let a = match self.execute(&RazerCommand::get_mode()) {
            Err(RazerError::Status(4)) => self.execute(&RazerCommand::get_mode_alternate())?,
            other => other?,
        };
        if !(a[0] == 0 || a[0] == 3) || a[1] != 0 {
            return Err(RazerError::Malformed("Unknown hardware mode. No change was made.".into()));
        }
        Ok(a[0])
    }

    pub fn read_battery(&mut self) -> Result<i32> {
        let a = self.execute(&RazerCommand::get_battery())?;
        Ok((a[1] as f64 * 100.0 / 255.0).round() as i32)
    }

    pub fn read_snapshot(&mut self) -> Result<HardwareSnapshot> {
        let mut warnings = Vec::new();
        let mut note = |label: &str, error: RazerError| warnings.push(format!("{label}: {error}"));
        let dpi = self.read_dpi().map_err(|e| note("DPI", e)).ok();
        let polling = self.read_polling().map_err(|e| note("Polling rate", e)).ok();
        let battery = self.read_battery().map_err(|e| note("Battery", e)).ok();
        let mode = self.read_mode().map_err(|e| note("Mode", e)).ok();
        if dpi.is_none() && polling.is_none() && battery.is_none() && mode.is_none() {
            return Err(RazerError::Transport(warnings.join("\n")));
        }
        Ok(HardwareSnapshot {
            dpi_x: dpi.map(|d| d.0),
            dpi_y: dpi.map(|d| d.1),
            polling_rate: polling,
            battery_level: battery,
            mode,
            warnings,
        })
    }

    pub fn set_dpi(&mut self, x: i32, y: i32) -> Result<()> {
        self.execute(&RazerCommand::set_dpi(x, y)?)?;
        if self.read_dpi()? != (x, y) {
            return Err(RazerError::Readback);
        }
        Ok(())
    }

    pub fn set_polling(&mut self, hz: i32) -> Result<()> {
        self.execute(&RazerCommand::set_polling(hz)?)?;
        if self.read_polling()? != hz {
            return Err(RazerError::Readback);
        }
        Ok(())
    }

    pub fn set_mode(&mut self, mode: u8) -> Result<()> {
        self.execute(&RazerCommand::set_mode(mode)?)?;
        if self.read_mode()? != mode {
            return Err(RazerError::Readback);
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::collections::VecDeque;

    pub fn reply(command: &RazerCommand, status: u8, arguments: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0u8; REPORT_LENGTH];
        bytes[0] = status;
        bytes[1] = command.transaction;
        bytes[5] = arguments.len() as u8;
        bytes[6] = command.class;
        bytes[7] = command.id;
        bytes[8..8 + arguments.len()].copy_from_slice(arguments);
        bytes[88] = checksum(&bytes);
        bytes
    }

    pub struct Scripted {
        pub replies: VecDeque<Result<Vec<u8>>>,
        pub requests: Vec<Vec<u8>>,
    }

    impl RazerTransport for Scripted {
        fn identity(&self) -> String {
            "1532:00b4:test".into()
        }
        fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>> {
            self.requests.push(request.to_vec());
            self.replies.pop_front().unwrap_or(Err(RazerError::Disconnected))
        }
    }

    #[test]
    fn encodes_known_commands() {
        let bytes = encode(&RazerCommand::get_dpi()).unwrap();
        assert_eq!(&bytes[..8], &[0, 0x1f, 0, 0, 0, 7, 4, 0x85]);
        assert_eq!(bytes[88], 7 ^ 4 ^ 0x85);
        let dpi = RazerCommand::set_dpi(1600, 800).unwrap();
        assert_eq!(dpi.arguments, vec![1, 6, 0x40, 3, 0x20, 0, 0]);
        assert!(RazerCommand::set_dpi(99, 800).is_err());
        assert!(RazerCommand::set_polling(250).is_err());
        assert_eq!(RazerCommand::set_polling(125).unwrap().arguments, vec![8]);
        assert!(RazerCommand::set_mode(1).is_err());
        assert!(encode(&RazerCommand::new(0x1f, 0, 0, vec![0; 81])).is_err());
    }

    #[test]
    fn decode_rejects_mismatches() {
        let command = RazerCommand::get_polling();
        assert_eq!(decode(&reply(&command, 2, &[1]), &command).unwrap(), vec![1]);
        assert_eq!(decode(&reply(&command, 4, &[1]), &command), Err(RazerError::Status(4)));
        let mut bad = reply(&command, 2, &[1]);
        bad[88] ^= 1;
        assert!(matches!(decode(&bad, &command), Err(RazerError::Malformed(_))));
        assert!(matches!(decode(&reply(&RazerCommand::get_dpi(), 2, &[0; 7]), &command), Err(RazerError::Malformed(_))));
        assert!(matches!(decode(&reply(&command, 2, &[1, 2]), &command), Err(RazerError::Malformed(_))));
        assert!(matches!(decode(&[0; 10], &command), Err(RazerError::Malformed(_))));
    }

    #[test]
    fn busy_replies_are_polled_without_resending() {
        let command = RazerCommand::get_polling();
        let request = encode(&command).unwrap();
        assert!(needs_response_poll(&reply(&command, 1, &[0]), &request));
        assert!(!needs_response_poll(&reply(&command, 2, &[1]), &request));
        assert!(needs_response_poll(&reply(&RazerCommand::get_dpi(), 2, &[0; 7]), &request), "stale reply");
        let read = RazerCommand::new(0x1f, 2, 0x8c, vec![1, 0x40, 0, 0, 0, 0, 0, 0, 0, 0]);
        let read_request = encode(&read).unwrap();
        let other = reply(&RazerCommand::new(0x1f, 2, 0x8c, vec![1, 0x41, 0, 0, 0, 0, 0, 0, 0, 0]), 2, &[1, 0x41, 0, 2, 2, 0, 4, 0, 0, 0]);
        assert!(needs_response_poll(&other, &read_request), "previous control's reply");
        let mut polls = vec![Ok(reply(&command, 1, &[0])), Ok(reply(&command, 2, &[1]))].into_iter();
        let result = finish_response(reply(&command, 1, &[0]), &request, &mut |_| {}, &mut || polls.next().unwrap()).unwrap();
        assert_eq!(result[0], 2);
        let error = finish_response(reply(&command, 1, &[0]), &request, &mut |_| {}, &mut || Ok(reply(&command, 1, &[0])));
        assert!(matches!(error, Err(RazerError::Transport(_))));
    }

    #[test]
    fn session_reads_snapshot_and_retries_busy() {
        let mut transport = Scripted {
            replies: VecDeque::from(vec![
                Ok(reply(&RazerCommand::get_dpi(), 1, &[0; 7])),
                Ok(reply(&RazerCommand::get_dpi(), 2, &[0, 6, 0x40, 6, 0x40, 0, 0])),
                Ok(reply(&RazerCommand::get_polling(), 2, &[1])),
                Ok(reply(&RazerCommand::get_battery(), 2, &[0, 237])),
                Ok(reply(&RazerCommand::get_mode(), 4, &[0, 0])),
                Ok(reply(&RazerCommand::get_mode_alternate(), 2, &[0, 0])),
            ]),
            requests: vec![],
        };
        let snapshot = Session::without_pauses(&mut transport).read_snapshot().unwrap();
        assert_eq!(snapshot.dpi_x, Some(1600));
        assert_eq!(snapshot.polling_rate, Some(1000));
        assert_eq!(snapshot.battery_level, Some(93));
        assert_eq!(snapshot.mode, Some(0));
        assert!(snapshot.warnings.is_empty());
        assert_eq!(transport.requests.len(), 6);
    }

    #[test]
    fn writes_are_verified() {
        let set = RazerCommand::set_polling(500).unwrap();
        let mut transport = Scripted {
            replies: VecDeque::from(vec![Ok(reply(&set, 2, &[2])), Ok(reply(&RazerCommand::get_polling(), 2, &[1]))]),
            requests: vec![],
        };
        assert_eq!(Session::without_pauses(&mut transport).set_polling(500), Err(RazerError::Readback));
    }
}
