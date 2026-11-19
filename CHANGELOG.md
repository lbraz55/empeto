# Changelog

## 0.2.0 — 2026-11-19 (schema do banco: 4) — EMPETO
Implementação das Fases 0 a 4 do documento "EMPETO — Documentação Técnica de Implementação v1.0".
- Fase 1: nova identidade EMPETO (nome, slogan, logo dos braços antes/depois, ícone), paleta monocromática,
  tema claro/escuro/sistema por tokens, tamanho de texto, densidade e estados vazios ilustrados.
- Fase 2: modo foco, cronômetro flutuante com anel, registrar com um toque (anterior/planejado),
  teclado numérico próprio com validação em Rust, feedback háptico, desfazer (5 s), barra de progresso da sessão.
- Fase 3: biblioteca com descrição/notas/favoritos/personalizados (migração 0004), filtros SQL e paginação,
  fichas favoritas na Home, reordenar por arrastar (setas mantidas), modelos ABC / Full Body / PPL.
- Fase 4: recordes pessoais calculados do histórico, gráfico de evolução interativo (tendência, seleção de ponto,
  eixo a partir de zero), mapa de intensidade no calendário (opcional), sugestão de progressão (nunca altera a ficha
  sozinha) e busca global centralizada.

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
