// Regras puras do sidecar (testadas em ferramentas/testes/sidecar-regras.test.mjs).

/**
 * "Lembrar aprovações": das sugestões que o Claude Code manda junto com o pedido de permissão, só passam
 * as regras de PERMITIR que valem até o fim DESTA sessão. Fica de fora:
 *  - setMode (acceptEdits, bypassPermissions, plan…): a aprovação da Ponte é sempre ligada;
 *  - qualquer destino que grave em arquivo (userSettings, projectSettings, localSettings);
 *  - addDirectories / removeRules / replaceRules e regra de negar ou perguntar.
 */
export function sugestoesSeguras(sugestoes) {
  if (!Array.isArray(sugestoes)) return []
  return sugestoes.filter((x) => x && x.type === 'addRules' && x.behavior === 'allow' && x.destination === 'session' && Array.isArray(x.rules) && x.rules.length > 0)
}
