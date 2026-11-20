# Changelog

## 0.4.0 — 2026-11-20 (schema do banco: 4) — Identidade e experiência inicial (Fase 1 do documento de evolução v1.1)
- **Idiomas**: português (Brasil), inglês e espanhol, com opção "Seguir o sistema" (Configurações › Interface ›
  Idioma). Catálogo único em `resources/i18n/<idioma>/LC_MESSAGES/workout.po`, usado pela interface Slint (`@tr`)
  e pelo Rust (`tr`/`trf!`). Nomes de exercícios, fichas e anotações nunca são traduzidos.
  `scripts/i18n.py check` (no CI) exige tradução de todas as frases.
- **Boas-vindas**: 4 etapas (apresentação, tour, personalização de idioma/tema/texto/animações, primeiros passos),
  com "Pular", "Criar minha primeira ficha" e "Conhecer o aplicativo primeiro". Aparece só no primeiro acesso
  (quem atualiza com dados não vê); pode ser refeita em Configurações › Sobre.
- **Abertura**: logo EMPETO no fundo da janela (Android 8–11) e SplashScreen do sistema (Android 12+), com
  esmaecimento curto no app; não atrasa a inicialização e respeita animações desativadas.
- **Tamanho da fonte**: Pequena, Média (padrão), Grande e Extragrande, multiplicado pela escala de fonte do Android.
  Preferência antiga migrada automaticamente (nova chave `ui.font_size`).
- **Temas**: cores semânticas sóbrias (sucesso, alerta, erro) como tokens, usadas só onde têm significado.
- **Animações**: indicador deslizante nas abas, barra de progresso animada, confirmação visual ao concluir série,
  entrada em cascata dos cartões da Home (decorativa, desligável).
- Números no formato do idioma (separador decimal e de milhar).

## 0.3.1 — 2026-11-20 (schema do banco: 4) — Estabilização (Fase 0 do documento de evolução v1.1)
- Recuperação de sessão interrompida: ao abrir o app com uma sessão não finalizada, o EMPETO pergunta se
  deseja continuar ou descartar (nada é concluído automaticamente; as séries registradas estão salvas).
- Banco com `synchronous=FULL`: cada série registrada sobrevive a desligamento repentino/bateria.
- CI: aviso explícito quando os Secrets da chave de assinatura de release não estão configurados
  (sem eles o APK não instala por cima da versão anterior).
- Teste de integração `interrupted_session_is_recovered_after_reopen`.

## 0.3.0 — 2026-11-19 (schema do banco: 4) — EMPETO Fases 5 e 6
- Fase 5 — Personalização:
  - Atalhos personalizáveis na Home (até 4, códigos estáveis, sem duplicados, restaurar padrão).
  - Recursos opcionais em Configurações: recordes, mapa de intensidade, sugestão de progressão, gestos,
    animações decorativas e lembrete de backup (desligar não apaga dados).
  - Gestos centralizados (`GestureEvent`): deslizar no modo foco (séries) e no calendário (período).
  - Controle de efeitos: animações decorativas (pulso do recorde, entrada dos estados vazios) desligáveis.
- Fase 6 — Armazenamento e backup:
  - Painel de espaço usado (banco, backups locais, temporários, total) e limpeza de temporários.
  - Tela Backups: lista com data e tamanho, criar agora, restaurar (mesma validação), excluir com confirmação,
    salvar cópia fora do aparelho.
  - Backup automático semanal ao abrir o app, mantendo os 4 mais recentes; lembrete de cópia externa.
  - Widget Android com o próximo treino / sessão em andamento (sem permissões novas).
- Documentação atualizada para EMPETO.

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
