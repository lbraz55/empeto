# Formato dos arquivos

## Planilha de importação (XLSX/ODS/XLS)

Uma linha por exercício. A primeira linha que parece um cabeçalho (entre as 15 primeiras) é usada.
Os cabeçalhos são reconhecidos sem diferenciar maiúsculas/acentos; colunas desconhecidas são
listadas na prévia e ignoradas. O mapeamento pode ser ajustado manualmente na prévia.

| Campo | Cabeçalhos reconhecidos (exemplos) | Obrigatório | Formato |
|---|---|---|---|
| Ficha | Ficha, Plano, Programa, Rotina | não (padrão: nome da aba) | texto; vazio herda a linha anterior |
| Dia | Dia, Treino, Divisão, Sessão, Letra | não (padrão: "Treino A") | texto; vazio herda a linha anterior |
| Dias da semana | Dias da semana, Semana | não | `Seg, Qui` / `segunda e quinta`. Se o nome do dia já for um dia da semana, ele é usado |
| Ordem | Ordem, Nº | não | informativo |
| Exercício | Exercício, Nome do exercício, Movimento | **sim** | texto; exercícios novos entram na biblioteca |
| Grupo muscular | Grupo muscular, Músculo, Foco | não | texto |
| Tipo | Tipo | não | texto |
| Equipamento | Equipamento, Aparelho | não | texto |
| Padrão de movimento | Padrão, Padrão de movimento | não | texto |
| Séries | Séries, Sets | não (padrão das preferências) | 1–50 |
| Repetições | Repetições, Reps | não (padrão das preferências) | `10`, `8-12`, `8 a 12`, `12/10/8` |
| Carga | Carga, Carga (kg), Peso | não | `40`, `42,5`, `20kg`, `45 lb` (sem unidade = unidade das preferências) |
| Descanso | Descanso, Intervalo | não (padrão das preferências) | segundos (`90`, `90s`), `1:30`, `1min30` |
| Observações | Observações, Obs, Notas | não | texto |

Organização da ficha: se todos os dias tiverem dias da semana, a ficha é "por dias da semana"; caso
contrário, "sequência".

Limites: arquivo ≤ 10 MB, ≤ 30 abas, ≤ 5.000 linhas por aba, ≤ 60 colunas.

Regras: nada é gravado antes da confirmação; erros impedem a importação; avisos (dados incompletos ou
inválidos substituídos pelo padrão) precisam ser conferidos; a gravação é uma transação única.

O app gera um **modelo** em Configurações › Dados e armazenamento › Baixar modelo de importação.

## Planilha de exportação

- Aba **Fichas**: mesmo layout da importação (pode ser reimportada).
- Aba **Sessões**: Data, Início, Fim, Ficha, Dia, Exercício planejado, Exercício realizado,
  Substituição (Temporária/Permanente), Série, Reps planejadas, Carga planejada (kg), Reps realizadas,
  Carga realizada (kg), Concluída às, Obs. do exercício, Obs. da sessão.
- Aba **Leia-me** com estas instruções.

Cargas exportadas sempre em kg (número).

## Backup (`workout-backup-AAAA-MM-DD.zip`)

```
manifest.json   {"format":"workout-backup","format_version":1,"app_version":"0.1.0",
                 "schema_version":2,"created_at":<epoch ms>,"db_sha256":"…","db_bytes":…,
                 "counts":{"plans":…,"exercises":…,"sessions":…,"sets":…}}
workout.db      cópia consistente do SQLite (VACUUM INTO)
```

Restauração: valida o zip, o manifesto, o SHA-256, `PRAGMA integrity_check`, a versão do schema
(backups de versões anteriores são migrados; de versões mais novas são recusados) e as chaves
estrangeiras. Só então substitui o banco atual, guardando o anterior em `workout.pre-restore.db`;
qualquer falha recoloca o banco anterior.

Compatibilidade: um app sempre restaura backups com `format_version` ≤ ao seu e `schema_version` ≤ ao seu.

## Calendário (`.ics`)

iCalendar 2.0 com sessões realizadas (com horário) e treinos planejados por dia da semana nas
próximas 8 semanas (dia inteiro). É uma cópia: não há sincronização contínua.

## Banco de dados

SQLite em `files/workout.db` (armazenamento privado). Datas em epoch ms UTC; cargas em gramas
inteiros. Migrações em `migrations/` aplicadas em transação; antes de migrar um banco existente é
feita uma cópia `workout.pre-migration-vN.db`.
