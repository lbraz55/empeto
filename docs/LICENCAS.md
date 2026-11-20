# Licenças

> **Decisão pendente do autor (documento de evolução v1.1, itens 17.15 e 17.16).** Este documento reúne o que
> foi verificado para apoiar a decisão; ele não define sozinho as permissões de reutilização.

## Situação atual do projeto

- O `Cargo.toml` declara `license = "GPL-3.0-only"`.
- Ainda **não há arquivo `LICENSE`** no repositório. Ele deve ser adicionado quando o autor confirmar a licença.
- Código, ilustrações (`assets/exercises`) e marca (`assets/branding`) foram feitos para o projeto.
  A marca EMPETO (nome e logotipo) normalmente fica fora da licença do código — vale deixar isso explícito.

## Slint (interface)

O Slint é oferecido sob três licenças, à escolha de quem distribui:

| Opção | O que significa para o EMPETO |
|---|---|
| **GPL-3.0** | Compatível com a licença declarada hoje. Quem distribuir o APK precisa oferecer o código-fonte sob GPL-3.0. |
| **Slint Royalty-free 2.0** | Permite apps de desktop/mobile sem custo, inclusive com código fechado, **exigindo atribuição** ao Slint (ex.: menção "Feito com Slint" na tela Sobre e no site/loja). |
| **Slint Software (comercial)** | Licença paga, para casos fora das anteriores. |

O app já mostra "Feito com Rust, Slint e SQLite" em Configurações › Sobre. Antes de distribuir publicamente (por
exemplo, na Play Store), confirme as condições atuais no site oficial do Slint.

## Dependências Rust

O inventário completo, gerado do `Cargo.lock`, está em [LICENCAS-TERCEIROS.md](LICENCAS-TERCEIROS.md)
(`scripts/licenses.py`). Resumo: a grande maioria é MIT e/ou Apache-2.0; há também Unicode-3.0, BSD, Zlib, ISC,
BSL-1.0 e CC0 — todas permissivas e compatíveis com GPL-3.0. Pacotes com opção LGPL (`r-efi`) também oferecem
MIT/Apache. O único componente com GPL é o Slint (com as alternativas acima).

## Outros componentes

- **SQLite**: domínio público (embutido via `rusqlite`/`libsqlite3-sys`).
- **Fontes do sistema**: o app usa as fontes do Android; nenhuma fonte é distribuída no APK.
- **Ícones e ilustrações**: desenhos próprios (traços SVG no código e em `assets/`).
- **Mochy-Music**: usado apenas como inspiração de conceitos (documento EMPETO v1.0); nenhum código ou arte copiado.
