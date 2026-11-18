//! Cronômetro de descanso (RF-10..RF-12, RN-06, RN-07, RNF-07).
//!
//! O estado guarda o *instante de término* em dois relógios:
//! - `end_boot_ms`: relógio monotônico que inclui o tempo de suspensão (CLOCK_BOOTTIME,
//!   o mesmo usado pelo AlarmManager `ELAPSED_REALTIME`). Imune a mudanças de horário.
//! - `end_wall_ms`: relógio de parede, usado somente se o aparelho foi reiniciado
//!   (o relógio monotônico recomeça do zero após o boot).
//!
//! O tempo restante é sempre recalculado a partir desses instantes, nunca por
//! decremento visual. O fim do descanso NÃO conclui série nem inicia outra (RN-06).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    pub boot_ms: i64,
    pub wall_ms: i64,
    /// Identificador do boot atual (hash de /proc/sys/kernel/random/boot_id; 0 se indisponível).
    pub boot_id: u64,
}

impl Clock {
    pub fn now() -> Clock {
        Clock { boot_ms: boot_ms(), wall_ms: chrono::Utc::now().timestamp_millis(), boot_id: boot_id() }
    }
}

fn boot_id() -> u64 {
    use std::sync::OnceLock;
    static ID: OnceLock<u64> = OnceLock::new();
    *ID.get_or_init(|| {
        std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .map(|s| {
                // FNV-1a
                s.trim().bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
            })
            .unwrap_or(0)
    })
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn boot_ms() -> i64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: ponteiro válido para timespec; CLOCK_BOOTTIME existe em Linux/Android.
    let r = unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut ts) };
    if r == 0 {
        ts.tv_sec as i64 * 1000 + ts.tv_nsec as i64 / 1_000_000
    } else {
        fallback_boot_ms()
    }
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn boot_ms() -> i64 {
    fallback_boot_ms()
}

fn fallback_boot_ms() -> i64 {
    use std::sync::OnceLock;
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_millis() as i64 + 1_000_000
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimerStatus {
    Idle,
    Running,
    Paused,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimerPhase {
    /// Descanso entre séries.
    Rest,
    /// Contagem de preparação antes do início da série (ação opcional configurada).
    Prepare,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerEvent {
    PreAlert,
    Finished(TimerPhase),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimerState {
    pub status: TimerStatus,
    pub phase: TimerPhase,
    pub duration_ms: i64,
    pub end_boot_ms: i64,
    pub end_wall_ms: i64,
    pub remaining_ms: i64,
    /// Valor do relógio monotônico no instante em que a contagem (re)começou.
    pub start_boot_ms: i64,
    pub boot_id: u64,
    pub pre_alert_ms: i64,
    pub pre_alert_done: bool,
    /// Contexto exibido: exercício e série relacionados.
    pub context: String,
}

pub const MIN_DURATION_MS: i64 = 1_000;
pub const MAX_DURATION_MS: i64 = 3 * 3600 * 1000;

impl Default for TimerState {
    fn default() -> Self {
        TimerState::new(90_000)
    }
}

impl TimerState {
    pub fn new(duration_ms: i64) -> TimerState {
        let d = duration_ms.clamp(MIN_DURATION_MS, MAX_DURATION_MS);
        TimerState {
            status: TimerStatus::Idle,
            phase: TimerPhase::Rest,
            duration_ms: d,
            end_boot_ms: 0,
            end_wall_ms: 0,
            remaining_ms: d,
            start_boot_ms: 0,
            boot_id: 0,
            pre_alert_ms: 0,
            pre_alert_done: false,
            context: String::new(),
        }
    }

    pub fn is_running(&self) -> bool {
        self.status == TimerStatus::Running
    }

    /// Define a duração. Se não estiver correndo, o restante passa a ser a duração.
    pub fn set_duration(&mut self, ms: i64) {
        self.duration_ms = ms.clamp(MIN_DURATION_MS, MAX_DURATION_MS);
        if self.status != TimerStatus::Running {
            self.remaining_ms = self.duration_ms;
            if self.status == TimerStatus::Finished {
                self.status = TimerStatus::Idle;
            }
            self.pre_alert_done = false;
        }
    }

    /// Inicia do zero (Idle/Finished) ou retoma (Paused).
    pub fn start(&mut self, now: Clock) {
        match self.status {
            TimerStatus::Running => {}
            TimerStatus::Paused => self.resume(now),
            TimerStatus::Idle | TimerStatus::Finished => {
                self.remaining_ms = self.duration_ms;
                self.pre_alert_done = false;
                self.run_from(now, self.duration_ms);
            }
        }
    }

    /// Reinicia a contagem completa e já começa a correr.
    pub fn restart(&mut self, now: Clock) {
        self.status = TimerStatus::Idle;
        self.start(now);
    }

    fn run_from(&mut self, now: Clock, remaining: i64) {
        self.status = TimerStatus::Running;
        self.end_boot_ms = now.boot_ms + remaining;
        self.end_wall_ms = now.wall_ms + remaining;
        self.start_boot_ms = now.boot_ms;
        self.boot_id = now.boot_id;
    }

    pub fn pause(&mut self, now: Clock) {
        if self.status == TimerStatus::Running {
            self.remaining_ms = self.remaining(now).max(0);
            self.status = TimerStatus::Paused;
        }
    }

    pub fn resume(&mut self, now: Clock) {
        if self.status == TimerStatus::Paused {
            let r = self.remaining_ms;
            self.run_from(now, r);
        }
    }

    /// Volta ao estado inicial com a duração configurada.
    pub fn reset(&mut self) {
        self.status = TimerStatus::Idle;
        self.remaining_ms = self.duration_ms;
        self.pre_alert_done = false;
        self.end_boot_ms = 0;
        self.end_wall_ms = 0;
    }

    /// Ajusta o tempo em +/- delta (RF-11).
    pub fn adjust(&mut self, now: Clock, delta_ms: i64) {
        match self.status {
            TimerStatus::Running => {
                let remaining = (self.remaining(now) + delta_ms).clamp(0, MAX_DURATION_MS);
                self.run_from(now, remaining);
                if remaining > self.pre_alert_ms {
                    self.pre_alert_done = false;
                }
                if remaining > self.duration_ms {
                    self.duration_ms = remaining;
                }
            }
            TimerStatus::Paused => {
                self.remaining_ms = (self.remaining_ms + delta_ms).clamp(0, MAX_DURATION_MS);
                if self.remaining_ms > self.duration_ms {
                    self.duration_ms = self.remaining_ms;
                }
            }
            TimerStatus::Idle | TimerStatus::Finished => {
                self.set_duration(self.duration_ms + delta_ms);
            }
        }
    }

    /// Tempo restante em ms (pode ser negativo se já passou do fim e ainda não houve tick).
    pub fn remaining(&self, now: Clock) -> i64 {
        match self.status {
            TimerStatus::Running => {
                if self.rebooted_since_start(now) {
                    self.end_wall_ms - now.wall_ms
                } else {
                    self.end_boot_ms - now.boot_ms
                }
            }
            TimerStatus::Paused | TimerStatus::Idle => self.remaining_ms,
            TimerStatus::Finished => 0,
        }
    }

    fn rebooted_since_start(&self, now: Clock) -> bool {
        // O relógio monotônico nunca volta: se voltou, houve reinicialização.
        // Quando disponível, o boot_id do kernel confirma a troca de boot.
        if self.boot_id != 0 && now.boot_id != 0 {
            return self.boot_id != now.boot_id;
        }
        now.boot_ms < self.start_boot_ms
    }

    /// Ajusta o estado após recarregar do armazenamento (app reaberto / processo encerrado).
    pub fn restore(&mut self, now: Clock) {
        if self.status == TimerStatus::Running && self.rebooted_since_start(now) {
            let remaining = self.end_wall_ms - now.wall_ms;
            self.run_from(now, remaining);
        }
    }

    /// Avança a máquina de estados; devolve eventos de alerta a emitir.
    pub fn tick(&mut self, now: Clock) -> Vec<TimerEvent> {
        let mut events = Vec::new();
        if self.status != TimerStatus::Running {
            return events;
        }
        let rem = self.remaining(now);
        if self.pre_alert_ms > 0
            && !self.pre_alert_done
            && self.duration_ms > self.pre_alert_ms
            && rem <= self.pre_alert_ms
            && rem > 0
        {
            self.pre_alert_done = true;
            events.push(TimerEvent::PreAlert);
        }
        if rem <= 0 {
            self.status = TimerStatus::Finished;
            self.remaining_ms = 0;
            self.pre_alert_done = true;
            events.push(TimerEvent::Finished(self.phase));
        }
        events
    }

    /// Fração concluída (0..1) para a barra de progresso.
    pub fn progress(&self, now: Clock) -> f32 {
        if self.duration_ms <= 0 {
            return 0.0;
        }
        let rem = self.remaining(now).clamp(0, self.duration_ms);
        1.0 - rem as f32 / self.duration_ms as f32
    }

    /// Texto mm:ss arredondando para cima (00:01 até o último instante).
    pub fn display(&self, now: Clock) -> String {
        let rem = self.remaining(now).max(0);
        let secs = (rem + 999) / 1000;
        super::format_duration_s(secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clk(boot: i64, wall: i64) -> Clock {
        Clock { boot_ms: boot, wall_ms: wall, boot_id: 7 }
    }

    #[test]
    fn counts_down_and_finishes() {
        let mut t = TimerState::new(60_000);
        t.start(clk(1_000, 1_000_000));
        assert_eq!(t.remaining(clk(31_000, 1_030_000)), 30_000);
        assert!(t.tick(clk(31_000, 1_030_000)).is_empty());
        let ev = t.tick(clk(61_000, 1_060_000));
        assert_eq!(ev, vec![TimerEvent::Finished(TimerPhase::Rest)]);
        assert_eq!(t.status, TimerStatus::Finished);
    }

    #[test]
    fn pause_does_not_advance() {
        // CT-07
        let mut t = TimerState::new(60_000);
        t.start(clk(0, 0));
        t.pause(clk(10_000, 10_000));
        assert_eq!(t.remaining(clk(500_000, 500_000)), 50_000);
        t.resume(clk(500_000, 500_000));
        assert_eq!(t.remaining(clk(510_000, 510_000)), 40_000);
    }

    #[test]
    fn background_recompute_and_wall_clock_change() {
        // CT-06: app suspenso — o tempo é recalculado pelo instante de término;
        // mudança no horário do sistema não afeta a contagem.
        let mut t = TimerState::new(90_000);
        t.start(clk(5_000, 1_000_000));
        let later = clk(35_000, 1_000_000 + 3_600_000 + 30_000);
        assert_eq!(t.remaining(later), 60_000);
        let ev = t.tick(clk(200_000, 1_195_000));
        assert_eq!(ev, vec![TimerEvent::Finished(TimerPhase::Rest)]);
    }

    #[test]
    fn reboot_falls_back_to_wall_clock() {
        let mut t = TimerState::new(120_000);
        t.start(clk(900_000, 10_000_000));
        // aparelho reiniciado: boot volta a ~0; passaram 50 s no relógio de parede.
        let after = Clock { boot_ms: 20_000, wall_ms: 10_050_000, boot_id: 8 };
        t.restore(after);
        assert_eq!(t.remaining(after), 70_000);
        assert_eq!(t.remaining(Clock { boot_ms: 30_000, wall_ms: 10_060_000, boot_id: 8 }), 60_000);
    }

    #[test]
    fn pre_alert_fires_once() {
        let mut t = TimerState::new(30_000);
        t.pre_alert_ms = 5_000;
        t.start(clk(0, 0));
        assert!(t.tick(clk(20_000, 20_000)).is_empty());
        assert_eq!(t.tick(clk(25_500, 25_500)), vec![TimerEvent::PreAlert]);
        assert!(t.tick(clk(26_000, 26_000)).is_empty());
    }

    #[test]
    fn adjust_and_reset() {
        let mut t = TimerState::new(60_000);
        t.adjust(clk(0, 0), 15_000);
        assert_eq!(t.duration_ms, 75_000);
        t.start(clk(0, 0));
        t.adjust(clk(10_000, 10_000), -15_000);
        assert_eq!(t.remaining(clk(10_000, 10_000)), 50_000);
        t.reset();
        assert_eq!(t.status, TimerStatus::Idle);
        assert_eq!(t.remaining(clk(99, 99)), 75_000);
    }

    #[test]
    fn finish_does_not_imply_anything_else() {
        // RN-06: o término apenas muda o status do cronômetro.
        let mut t = TimerState::new(1_000);
        t.start(clk(0, 0));
        let ev = t.tick(clk(2_000, 2_000));
        assert_eq!(ev, vec![TimerEvent::Finished(TimerPhase::Rest)]);
        assert!(t.tick(clk(3_000, 3_000)).is_empty());
    }
}
