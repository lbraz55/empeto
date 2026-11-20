# Desempenho (documento de evolução v1.1, seção 12)

## Como medir

```bash
cargo test --release --test bench -- --ignored --nocapture
```

O teste `tests/bench.rs` cria bancos com **100, 1.000 e 10.000 sessões** (5 exercícios × 4 séries cada) e mede
as operações mais usadas, com as mesmas configurações do app (WAL, `synchronous=FULL`). Ele não roda no
`cargo test` normal.

## Resultados (versão 0.7.0, desktop x86_64, build release)

Em celulares intermediários espere tempos de 2 a 4 vezes maiores. Para referência: 10.000 sessões equivalem
a cerca de 27 anos treinando todos os dias.

```
100 sessões (gerar: 0.2 s; arquivo: 0.5 MB)
  abrir banco (inicialização)                       0.6 ms
  histórico: primeira página (100 sessões)          1.5 ms
  histórico: todas as sessões (referência)          1.3 ms
  histórico: exercícios realizados                  0.9 ms
  evolução de um exercício (tudo)                   0.4 ms
  relatório mensal                                  3.3 ms
  recordes dos últimos 30 dias                      2.0 ms
  estatísticas de 2 sessões (comparação)            0.3 ms
  busca global                                      0.5 ms
  abrir fichas                                      0.2 ms
  iniciar sessão                                    2.6 ms
  registrar série                                   0.4 ms
  recorde da série registrada                       0.3 ms
  criar backup                                     12.0 ms
  backup: 0.1 MB
1000 sessões (gerar: 1.9 s; arquivo: 3.9 MB)
  abrir banco (inicialização)                       0.5 ms
  histórico: primeira página (100 sessões)          1.4 ms
  histórico: todas as sessões (referência)         12.5 ms
  histórico: exercícios realizados                  5.4 ms
  evolução de um exercício (tudo)                   3.2 ms
  relatório mensal                                 22.3 ms
  recordes dos últimos 30 dias                     20.3 ms
  estatísticas de 2 sessões (comparação)            0.4 ms
  busca global                                      1.9 ms
  abrir fichas                                      0.1 ms
  iniciar sessão                                    2.5 ms
  registrar série                                   0.5 ms
  recorde da série registrada                       2.6 ms
  criar backup                                     71.1 ms
  backup: 0.4 MB
10000 sessões (gerar: 17.6 s; arquivo: 39.3 MB)
  abrir banco (inicialização)                       0.5 ms
  histórico: primeira página (100 sessões)          1.6 ms
  histórico: todas as sessões (referência)        128.1 ms
  histórico: exercícios realizados                 70.9 ms
  evolução de um exercício (tudo)                  39.1 ms
  relatório mensal                                231.2 ms
  recordes dos últimos 30 dias                    231.1 ms
  estatísticas de 2 sessões (comparação)            0.4 ms
  busca global                                     17.3 ms
  abrir fichas                                      0.1 ms
  iniciar sessão                                    3.7 ms
  registrar série                                   0.3 ms
  recorde da série registrada                      28.6 ms
  criar backup                                    671.7 ms
  backup: 3.4 MB
```

## O que foi otimizado nesta versão

| Gargalo encontrado | Antes (10.000 sessões) | Depois | Mudança |
|---|---|---|---|
| Busca global | 227 ms | 17 ms | uma única consulta com os textos das sessões; contagem de séries só para os resultados |
| Lista do histórico | 128 ms (carregava tudo) | 1,6 ms | paginação de 100 em 100 com "Carregar mais" (12.1: não carregar o histórico inteiro) |
| Relatórios e evolução | — | — | índices `workout_session(status, started_at)` e `session_exercise(performed_exercise_id)` (migração 0007) |

## Pontos conhecidos (ainda aceitáveis)

- **Recordes recentes / relatório mensal (≈230 ms com 10.000 sessões)**: o cálculo de recordes percorre o
  histórico em ordem para saber o que era recorde em cada data. Com até alguns milhares de sessões fica abaixo
  de 25 ms. Próximo passo, se necessário: guardar um cache derivado das melhores marcas por exercício
  (recalculável a partir do histórico, sem virar segunda fonte de verdade).
- **Backup (≈0,7 s com 10.000 sessões)**: roda ao tocar em "Criar backup" e no backup automático semanal ao abrir
  o app. Se crescer, mover para uma tarefa em segundo plano com indicador de progresso.
- **Registro de série**: 0,3–0,5 ms em todos os tamanhos (gravação durável com `synchronous=FULL`).
