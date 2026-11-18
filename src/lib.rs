//! WORKOUT — organizador pessoal de treinos, offline-first.
//!
//! Arquitetura (seção 3.2 da documentação):
//! - `domain`        entidades, regras e cálculos (inclui o cronômetro)
//! - `persistence`   SQLite, migrações versionadas, preferências
//! - `training`      fichas, exercícios, sessões em andamento
//! - `history`       sessões concluídas e evolução
//! - `calendar`      datas, planejado × realizado
//! - `import_export` XLSX, backup, ICS
//! - `application`   casos de uso/preferências
//! - `platform`      integração Android (notificações, vibração, alarmes, seletor de arquivos)
//! - `app`           interface Slint

pub mod application;
pub mod calendar;
pub mod domain;
pub mod history;
pub mod import_export;
pub mod persistence;
pub mod training;
