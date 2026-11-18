# Changelog

## 0.1.4 — 2026-11-18 (schema do banco: 3)
- Transições animadas entre telas (direção conforme a navegação), diálogos e avisos.
- Cadastro manual de alternativas de exercício (RF-09).
- Descanso cumprido gravado por série (migração 0003), exibido no histórico e na exportação.
- Senha opcional (AES-256) no backup.
- Importar todas as abas da planilha de uma vez.
- "Agenda": adiciona treino ao calendário do aparelho via Intent, sem permissão.
- Diagnóstico técnico em Configurações › Sobre.

## 0.1.1 – 0.1.3
- Tela de diagnóstico de abertura, rastros de inicialização, captura de falhas nativas.
- Corrigida a falha ao abrir (NDK 30 obrigatório; verificação de símbolos no build).

## 0.1.0 — 2026-11-18 (schema do banco: 2)

Primeira versão funcional, implementando a documentação v0.2:
- Fichas (dias da semana / sequência), biblioteca de exercícios, sessão com registro real por série,
  substituição temporária/permanente, snapshot e histórico imutável.
- Cronômetro em tela própria com alertas configuráveis e funcionamento em segundo plano.
- Home com cartões, calendário interno (mês/semana), histórico, evolução por exercício.
- Importação/exportação XLSX com prévia e mapeamento, backup/restauração, exportação ICS.
- Permissões: somente POST_NOTIFICATIONS e VIBRATE.
