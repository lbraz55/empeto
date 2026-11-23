# Guia de instalação

## Requisitos

- Android **8.0 (API 26)** ou mais novo.
- Cerca de **40 MB** livres (app + dados).
- Internet só para baixar o APK e, se você quiser, para verificar atualizações. O uso do app é offline.

## Baixar

1. Abra a página de **[Releases](https://github.com/lbraz55/empeto/releases/latest)** no celular.
2. Em *Assets*, toque no arquivo `empeto-<versão>-release.apk` (ou `…-debugsigned.apk` em versões de teste).
   - O APK publicado inclui **arm64-v8a** (praticamente todos os celulares atuais) e **x86_64** (emuladores).
     Não é preciso escolher arquitetura.

## Instalar

1. Abra o arquivo baixado (pela notificação do navegador ou pelo app *Arquivos*).
2. Na primeira vez, o Android pede para **permitir instalar apps desta fonte** (navegador/Arquivos). Permita e
   volte.
3. Toque em **Instalar**.

## Atualizar

- **Pelo próprio app** (desde 0.8.0): Configurações › Sobre › Atualizações › "Verificar agora" → "Baixar e
  instalar" → confirmar. O EMPETO confere o arquivo (SHA-256, pacote, versão e assinatura) antes.
- **Manualmente**: baixe o APK novo da Release e instale por cima. Os dados são mantidos.
- Se o Android disser que o pacote **conflita** com o existente, o APK foi assinado com outra chave (veja
  abaixo). Para trocar, **faça um backup em arquivo** (Configurações › Backups › Salvar backup em arquivo),
  desinstale, instale o novo e restaure o backup.

## Preservar os dados

- Os treinos ficam só no aparelho. Desinstalar o app **apaga** os dados.
- O app faz um **backup automático semanal** no próprio aparelho (mantém os 4 últimos), mas ele também é apagado
  na desinstalação. Use **Salvar cópia** / **Salvar backup em arquivo** para guardar fora do aparelho.

## Problemas comuns

| Problema | O que fazer |
|---|---|
| "App não instalado" / conflito de pacote | Assinatura diferente da versão instalada: backup em arquivo → desinstalar → instalar → restaurar. |
| O app fecha ao abrir | Abra de novo. Se continuar, veja Configurações › Sobre › Diagnóstico técnico e envie o texto por e-mail (Configurações › Ajuda e suporte). |
| Alerta do descanso não toca com a tela bloqueada | Permita notificações (Configurações › Sons e alertas) e desative a otimização de bateria para o EMPETO, se o seu Android for agressivo. |
| "Permitir desta fonte" não aparece | Configurações do Android › Apps › Acesso especial › Instalar apps desconhecidos. |
| Atualização recusada por assinatura | A versão nova foi assinada com outra chave; instale manualmente (passos acima). |

## Para quem mantém o projeto: assinatura

Para que cada versão instale **por cima** da anterior, o GitHub Actions precisa assinar sempre com a mesma chave.
Cadastre em *Settings › Secrets and variables › Actions*: `KEYSTORE_BASE64`, `KEYSTORE_PASSWORD`, `KEY_ALIAS`,
`KEY_PASSWORD`. Sem eles o workflow avisa e usa uma chave temporária. A keystore nunca deve ir para o repositório.
