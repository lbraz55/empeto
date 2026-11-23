# Changelog

## 0.9.1 — 2026-11-23 (schema do banco: 7)
- **Removida a tela "O EMPETO não abriu corretamente da última vez"**: ela aparecia por engano em aberturas
  normais (o tratador de falhas nativas criava um `crash.txt` vazio a cada abertura e a tela tratava a simples
  existência do arquivo como falha). Agora o app sempre abre direto; erros registrados, se houver, ficam em
  Configurações › Sobre › Diagnóstico técnico.
- **Nova tela de carregamento**: o logo EMPETO aparece suavemente, fica um instante na tela e depois a tela
  esmaece revelando o app (com animações desligadas, o logo aparece sem efeitos e some logo em seguida).

## 0.9.0 — 2026-11-20 (schema do banco: 7) — Organização e publicação (Fase 6 do documento de evolução v1.1)
- **Ajuda e suporte** no app (Configurações): 10 perguntas frequentes e botão "Fale conosco por e-mail"
  (iempetofaq@gmail.com, assunto "Suporte EMPETO"), que só abre o app de e-mail — nada é enviado sozinho.
- **README** novo e visual: logo, slogan, capturas de tela, GIF das boas-vindas, funcionalidades, download,
  FAQ resumido, permissões, tecnologias e links para a documentação.
- Novos guias: [Instalação](docs/INSTALACAO.md), [Desenvolvimento e reutilização](docs/DESENVOLVIMENTO.md),
  [FAQ](docs/FAQ.md), [Licenças](docs/LICENCAS.md) + inventário gerado das dependências
  ([LICENCAS-TERCEIROS.md](docs/LICENCAS-TERCEIROS.md), `scripts/licenses.py`).
- Diagrama de arquitetura (Mermaid) em docs/ARQUITETURA.md; capturas em docs/images.

## 0.8.0 — 2026-11-20 (schema do banco: 7) — Integrações e distribuição (Fase 5 do documento de evolução v1.1)
- **Verificação de atualizações** (Configurações › Sobre › Atualizações): consulta o `empeto-update.json` da
  Release oficial no GitHub sob demanda ou automaticamente no máximo 1x/semana (desligável). Mostra versão
  instalada e disponível, data, tamanho e novidades; cartão na Home quando há versão nova.
- **Instalação assistida e segura**: só baixa quando você toca em "Baixar e instalar"; aceita apenas links da
  Release oficial; confere tamanho e SHA-256, que o arquivo é o EMPETO, de versão maior e **assinado com a mesma
  chave** do app instalado; entrega ao instalador do Android, que pede confirmação (nunca silencioso). Dados
  locais preservados.
- **WellHub** (atalho opcional da Home, "WellHub (check-in)"): abre o destino oficial do WellHub; o Android abre
  o app instalado se ele tratar o endereço. Não acessa conta, não guarda credenciais e não faz check-in.
- **Permissões revisadas**: novas `INTERNET` (só atualizações) e `REQUEST_INSTALL_PACKAGES` (só para entregar a
  atualização confirmada ao instalador); tráfego sem HTTPS bloqueado (`usesCleartextTraffic=false`).
- CI publica `empeto-update.json` (versão, código, notas do CHANGELOG, URL, tamanho, SHA-256 e ABIs de cada APK).
- Texto "Sobre" atualizado e traduzido.

## 0.7.0 — 2026-11-20 (schema do banco: 7) — Análise e evolução (Fase 4 do documento de evolução v1.1)
- **RPE/RIR** (opcional, Configurações › Treinamento): esforço percebido (1–10, meios pontos) e repetições em
  reserva por série; aparecem no detalhe da sessão, na comparação e no relatório (RPE médio).
- **Comparação de sessões**: no detalhe de uma sessão, "Comparar com outra sessão" mostra lado a lado data,
  duração, séries, volume e, por exercício, séries, carga máxima, repetições, volume e 1RM estimada, com as
  diferenças destacadas (seta + cor). Sessões do mesmo dia da ficha aparecem primeiro.
- **Relatório mensal** (Histórico › Mensal): treinos, dias com treino, duração total/média, exercícios diferentes,
  séries, volume, recordes, RPE e avaliação médios, sessões por semana e séries por grupo muscular. Agregações no
  SQLite. "Compartilhar resumo" envia um texto pelo seletor do Android (opcional).
- **Diário de treinamento** (Histórico › Diário): "Como foi o treino?" (muito ruim … excelente) durante a sessão,
  junto das observações da sessão e dos exercícios.
- **Desempenho**: benchmark com 100/1.000/10.000 sessões (`tests/bench.rs`, resultados em docs/DESEMPENHO.md);
  busca global 13× mais rápida com muitos dados; histórico carregado de 100 em 100; novos índices.
- Migração 0007 (`session_set.rpe/rir`, `workout_session.feeling` imutável após concluir, índices).

## 0.6.0 — 2026-11-20 (schema do banco: 6) — Eficiência de treinamento (Fase 3 do documento de evolução v1.1)
- **Valores da sessão anterior**: cartão "SESSÃO ANTERIOR · data" com carga × repetições de cada série do mesmo
  exercício, e ação "Repetir valores da sessão anterior". Nada é preenchido sem pedido: "Sugerir carga da última
  sessão" passa a vir desligado em instalações novas (quem já usava mantém a escolha).
- **Tipos de série**: normal, aquecimento, drop set, até a falha, back-off e técnica especial. Planejados na ficha
  (toque no número da série) e ajustáveis na sessão (toque em "Série N"); registrados no histórico, no detalhe da
  sessão e na exportação. Aquecimento não conta para recordes nem para a sugestão de progressão.
- **Superséries, trisséries e circuitos**: botão de corrente no editor liga um exercício ao seguinte. Na sessão, ao
  registrar uma série o app vai direto ao próximo exercício do grupo sem descanso; o descanso vem depois do último,
  voltando ao primeiro para a rodada seguinte. Cada exercício mantém seus próprios registros.
- **Calculadora de anilhas**: monta a carga com o menor número de anilhas, respeita barra e pares disponíveis
  (configuráveis) e avisa quando a carga não pode ser montada exatamente.
- **Calculadora de aquecimento**: séries a partir de percentuais e repetições configuráveis, arredondadas pelo
  incremento de carga (ferramenta de organização, não prescrição).
- Migração 0006 (`plan_exercise.set_kinds`, `plan_exercise.group_tag`, `session_exercise.group_tag`,
  `session_set.set_kind`); sessões antigas ficam com tipo "normal". Exportação: colunas "Tipo da série" e "Grupo".

## 0.5.0 — 2026-11-20 (schema do banco: 5) — Biblioteca visual (Fase 2 do documento de evolução v1.1)
- **Ilustrações próprias** dos exercícios (25 desenhos monocromáticos + genérica) em `assets/exercises/`,
  embutidas no app (funcionam offline) e recoloridas pelo tema claro/escuro. Aparecem na biblioteca, nos
  detalhes, na sessão e no modo foco. Exercícios da biblioteca inicial já vêm ilustrados; personalizados usam
  a ilustração escolhida ou uma automática pelo padrão de movimento.
- **Tela de detalhes do exercício** (tocar no exercício na biblioteca): ilustração, grupo/tipo/equipamento,
  técnica, observações pessoais, último registro, melhores marcas, alternativas e atalho para a evolução.
- **Vídeo demonstrativo**: link externo opcional (https) cadastrado pelo usuário, aberto no app/navegador do
  aparelho. O EMPETO não baixa nem redistribui vídeos e continua sem permissão de internet.
- Migração 0005 (`exercise.illustration`, `exercise.video_url`); backups antigos continuam restauráveis.
- Gerador das ilustrações em `scripts/gen_exercise_art.py`.

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
