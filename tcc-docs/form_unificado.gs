/**
 * RAVEN — Formulário Unificado
 * Pré-Teste → Atividade Guiada → Pós-Teste
 *
 * INSTRUÇÕES:
 *   1. Acesse https://script.google.com e crie um novo projeto.
 *   2. Cole todo este arquivo no editor.
 *   3. Clique em "Executar" (▶) ou pressione Ctrl+R.
 *   4. Autorize as permissões quando solicitado.
 *   5. O formulário (4 seções) será criado no seu Google Drive.
 *   6. O link aparecerá no painel "Execuções" (Logs).
 *
 * ESTRUTURA:
 *   Seção 1 — Perfil do Participante
 *   Seção 2 — Pré-Teste (Q1–Q12 com "Não sei")
 *   Seção 3 — Atividade Guiada no RAVEN (D1–D6, ~55 min)
 *   Seção 4 — Pós-Teste (Q1–Q12 sem "Não sei")
 */

function criarValidacaoPorRegex(pattern, helpText) {
  return FormApp.createTextValidation()
    .requireTextMatchesPattern(pattern)
    .setHelpText(helpText)
    .build();
}

function criarValidacaoNumeroDecimal(helpText) {
  return criarValidacaoPorRegex(
    '^-?\\d+(?:[.,]\\d+)?$',
    helpText || 'Informe apenas um valor numérico. Use ponto ou vírgula para decimais.'
  );
}

function criarValidacaoInteiro(min, max, helpText) {
  return FormApp.createTextValidation()
    .requireNumberBetween(min, max)
    .setHelpText(helpText || ('Informe um número inteiro entre ' + min + ' e ' + max + '.'))
    .build();
}

function criarFormulario() {

  var form = FormApp.create('RAVEN — Avaliação Didática');
  form.setDescription(
    'Este formulário faz parte de uma pesquisa sobre o simulador educacional RAVEN (RISC-V). ' +
    'As respostas são anônimas e utilizadas exclusivamente para fins de pesquisa.\n\n' +
    'Como participar:\n' +
    '1) Preencha o breve perfil do participante.\n' +
    '2) Responda o Pré-Teste (questionário inicial).\n' +
    '3) Realize a Atividade Guiada seguindo o tutorial no simulador RAVEN.\n' +
    '4) Responda o Pós-Teste (questionário final).\n\n' +
    'Duração aproximada: 80 a 90 minutos.'
  );
  form.setCollectEmail(false);
  form.setShowLinkToRespondAgain(false);

  // ═══════════════════════════════════════════════════════════════════════════
  // SEÇÃO 1 — Perfil do Participante
  // ═══════════════════════════════════════════════════════════════════════════

  form.addSectionHeaderItem().setTitle('Seção 1 — Perfil do Participante');

  // Curso
  var p2 = form.addMultipleChoiceItem();
  p2.setTitle('Curso de graduação');
  p2.setRequired(true);
  p2.setChoices([
    p2.createChoice('Ciência da Computação'),
    p2.createChoice('Engenharia de Computação'),
    p2.createChoice('Sistemas de Informação')
  ]);
  p2.showOtherOption(true);

  // Período
  var p3 = form.addTextItem();
  p3.setTitle('Qual é o seu período atual no curso?');
  p3.setHelpText('Exemplo: 3, 5, 7 — informe apenas o número.');
  p3.setValidation(
    criarValidacaoInteiro(1, 12, 'Informe apenas o número do período, entre 1 e 12.')
  );
  p3.setRequired(true);

  // Status de Arquitetura de Computadores
  var p4a = form.addMultipleChoiceItem();
  p4a.setTitle('Você já cursou Arquitetura de Computadores ou Organização de Computadores?');
  p4a.setRequired(true);
  p4a.setChoices([
    p4a.createChoice('Não'),
    p4a.createChoice('Sim, estou cursando agora'),
    p4a.createChoice('Sim, já concluí')
  ]);

  // Assuntos já estudados
  var p4b = form.addCheckboxItem();
  p4b.setTitle('Quais assuntos você já estudou na universidade?');
  p4b.setHelpText('Marque todos os que se aplicam, independentemente da disciplina.');
  p4b.setRequired(false);
  p4b.setChoices([
    p4b.createChoice('Arquitetura geral da CPU (registradores, memória, ciclo de instrução, ISA - Instruction Set Architecture)'),
    p4b.createChoice('Formatos de instrução RISC-V ou outro ISA (Instruction Set Architecture) (R, I, S, B, U, J-type)'),
    p4b.createChoice('Pipeline de processadores (estágios IF, ID, EX, MEM, WB)'),
    p4b.createChoice('Hazards de dados e controle (RAW - Read-After-Write, WAR - Write-After-Read, WAW - Write-After-Write, forwarding, stall, flush)'),
    p4b.createChoice('Cache e hierarquia de memória (mapeamento, políticas LRU - Least Recently Used / FIFO - First In, First Out, AMAT - Average Memory Access Time)'),
    p4b.createChoice('Medidas de desempenho (CPI - Cycles Per Instruction, AMAT - Average Memory Access Time, speedup, lei de Amdahl)'),
    p4b.createChoice('Processamento paralelo / hardware threads / multi-core'),
    p4b.createChoice('Nenhum dos assuntos acima')
  ]);

  // Simulador prévio
  var p5 = form.addMultipleChoiceItem();
  p5.setTitle('Você já usou algum simulador de CPU antes desta sessão?');
  p5.setRequired(true);
  p5.setChoices([
    p5.createChoice('Não'),
    p5.createChoice('Sim')
  ]);

  // ═══════════════════════════════════════════════════════════════════════════
  // SEÇÃO 2 — Pré-Teste
  // ═══════════════════════════════════════════════════════════════════════════

  var pg2 = form.addPageBreakItem();
  pg2.setTitle('Seção 2 — Pré-Teste');
  pg2.setHelpText(
    'Responda com base no seu conhecimento atual, sem consultar materiais ou o simulador. ' +
    'As questões avaliam compreensão de conceitos, não realização de cálculos. ' +
    'A opção "E) Não sei" está disponível e deve ser usada quando não houver certeza — ' +
    'ela é parte do instrumento de avaliação. Tempo estimado: 15 minutos.'
  );

  var q1 = form.addMultipleChoiceItem();
  q1.setTitle('Questão 1 — Formatos de instrução RISC-V\n\nQuais formatos de instrução RISC-V não possuem o campo rd (registrador de destino) em sua codificação binária?');
  q1.setRequired(true);
  q1.setChoices([
    q1.createChoice('A) R-type (add, sub, mul) e I-type (addi, lw, jalr) — porque usam dois operandos fonte e poderiam reaproveitar o espaço de rd para ampliar o imediato.'),
    q1.createChoice('B) S-type (sw, sb) e B-type (beq, bne) — porque produzem efeito em memória ou no PC e não escrevem em registrador de destino. Nesses formatos, os bits [11:7] são reaproveitados para codificar parte do imediato.'),
    q1.createChoice('C) U-type (upper immediate) e J-type (jump) — porque o imediato de 20 bits ocupa quase toda a instrução e empurra o campo rd para fora da codificação útil.'),
    q1.createChoice('D) Apenas ECALL e EBREAK — porque instruções de sistema são tratadas por um caminho especial de controle e, por isso, dispensam qualquer registrador de destino.'),
    q1.createChoice('E) Não sei')
  ]);

  var q4 = form.addMultipleChoiceItem();
  q4.setTitle('Questão 2 — Função do campo tag no endereçamento de cache\n\nUm endereço de memória é dividido em: offset (seleciona o byte dentro do bloco), índice (seleciona o set da cache) e tag (bits de alta ordem). Qual desses campos é usado para verificar se o bloco armazenado em um determinado set corresponde ao endereço sendo acessado?');
  q4.setRequired(true);
  q4.setChoices([
    q4.createChoice('A) O campo índice, porque ele identifica unicamente o set e — em mapeamento direto — também identifica unicamente o bloco armazenado naquele set.'),
    q4.createChoice('B) O campo offset, porque indica a posição exata dentro do bloco, garantindo que os bytes corretos sejam retornados ao processador.'),
    q4.createChoice('C) O campo tag, porque múltiplos endereços distintos mapeiam para o mesmo set. A tag carrega os bits de alta ordem e permite confirmar se o bloco presente naquele set pertence ao endereço sendo acessado.'),
    q4.createChoice('D) Nenhum campo adicional é necessário em mapeamento direto — o índice já garante que o bloco é o correto, pois cada set só pode conter dados de um único endereço por vez.'),
    q4.createChoice('E) Não sei')
  ]);

  var q5 = form.addMultipleChoiceItem();
  q5.setTitle('Questão 3 — Limitação da política LRU em varredura sequencial\n\nUm aluno argumenta que LRU (Least Recently Used) é sempre a melhor escolha porque "o que foi usado mais recentemente provavelmente será usado de novo." Por que essa lógica falha para programas que fazem varreduras sequenciais de arrays grandes (padrão streaming)?');
  q5.setHelpText('Nesta questão, FIFO significa First In, First Out.');
  q5.setRequired(true);
  q5.setChoices([
    q5.createChoice('A) Falha porque o LRU exige mais hardware que FIFO (First In, First Out) — esse overhead de manutenção do rank pode anular o ganho esperado em hit rate quando o array é muito grande.'),
    q5.createChoice('B) Falha porque em streaming o programa percorre os blocos em ordem e quase não volta a usá-los no curto prazo. Sem localidade temporal, o LRU preserva linhas que já cumpriram seu papel e não serão reutilizadas tão cedo.'),
    q5.createChoice('C) Falha apenas quando o array não cabe inteiramente na cache — se couber, LRU passa a ser automaticamente a melhor política para qualquer padrão de acesso sequencial.'),
    q5.createChoice('D) Falha porque varreduras sequenciais ativam um modo de prefetch automático no hardware que contorna a política LRU, tornando a escolha de política praticamente irrelevante.'),
    q5.createChoice('E) Não sei')
  ]);

  var q6 = form.addMultipleChoiceItem();
  q6.setTitle('Questão 4 — AMAT (Average Memory Access Time / tempo médio de acesso à memória) e o trade-off entre tamanho e latência de cache\n\nDois projetos de cache L1:\n• Config A: 16 KB, 4-way, hit time = 1 ciclo, miss rate = 8%\n• Config B: 64 KB, 4-way, hit time = 4 ciclos, miss rate = 6%\nAmbas com miss penalty = 50 ciclos. Apesar de a Config B ter miss rate menor, seu AMAT é maior. Como isso é possível?');
  q6.setRequired(true);
  q6.setChoices([
    q6.createChoice('A) AMAT = Hit Time + Miss Rate × Miss Penalty. A Config B sofre menos misses, mas cada hit custa muito mais; se esse aumento de hit time superar o ganho na miss rate, o AMAT total piora mesmo em uma cache maior.'),
    q6.createChoice('B) É um erro de configuração — hit time acima de 1 ciclo não deveria aparecer em L1, então a diferença observada indica parâmetros incoerentes ou entrada incorreta no experimento.'),
    q6.createChoice('C) Indica que a miss penalty domina completamente o cálculo — por isso diferenças de hit time quase não pesam no AMAT final quando o programa acessa memória com frequência.'),
    q6.createChoice('D) Caches maiores sempre produzem AMAT menor, independentemente do hit time — a fórmula AMAT = Hit Time + Miss Rate × Miss Penalty não captura esse efeito, portanto a comparação entre Config A e B por AMAT não tem validade.'),
    q6.createChoice('E) Não sei')
  ]);

  var q8 = form.addMultipleChoiceItem();
  q8.setTitle('Questão 5 — Função do estágio WB no pipeline de cinco estágios\n\nNo pipeline: IF → ID → EX → MEM → WB. Qual é a função primária do estágio WB (Write-Back)?');
  q8.setRequired(true);
  q8.setChoices([
    q8.createChoice('A) Ler os operandos do banco de registradores (rs1, rs2) e detectar dependências de dados entre instruções em voo.'),
    q8.createChoice('B) Escrever o resultado da operação no registrador de destino (rd) especificado pela instrução.'),
    q8.createChoice('C) Buscar a próxima instrução na memória de instruções usando o valor atual do PC.'),
    q8.createChoice('D) Executar a operação aritmética ou lógica na ULA sobre os operandos preparados no estágio anterior.'),
    q8.createChoice('E) Não sei')
  ]);

  var q9 = form.addMultipleChoiceItem();
  q9.setTitle('Questão 6 — Pipeline aumenta throughput, não reduz latência individual\n\nUm aluno afirma: "O pipeline acelera o processamento porque cada instrução individual é executada mais rápido." Por que essa afirmação está incorreta?');
  q9.setRequired(true);
  q9.setChoices([
    q9.createChoice('A) Está incorreta porque o pipeline reduz a frequência de clock para acomodar os múltiplos estágios, e o ganho de desempenho vem apenas do paralelismo entre programas diferentes rodando simultaneamente.'),
    q9.createChoice('B) Está incorreta porque o ganho de desempenho vem exclusivamente do cache de instruções (I-Cache) — o pipeline em si não contribui.'),
    q9.createChoice('C) Está incorreta porque o pipeline aumenta o throughput (instruções finalizadas por unidade de tempo) ao sobrepor execução de múltiplas instruções em estágios distintos, mas a latência individual de cada instrução permanece a mesma ou até aumenta levemente.'),
    q9.createChoice('D) Está incorreta porque o pipeline só acelera instruções aritméticas; instruções de memória e branches têm a mesma latência em execução pipelined e sequencial.'),
    q9.createChoice('E) Não sei')
  ]);

  var q10 = form.addMultipleChoiceItem();
  q10.setTitle('Questão 7 — Load-Use Hazard com forwarding ativo\n\nCom pipeline e forwarding ativados, o seguinte trecho é executado:\n  lw  x5, 0(x1)   ← carrega da memória para x5\n  add x6, x5, x2  ← usa x5 imediatamente\nUma bolha é inserida entre as duas instruções. Qual é a causa correta?');
  q10.setHelpText('Nesta questão, WAW significa Write-After-Write.');
  q10.setRequired(true);
  q10.setChoices([
    q10.createChoice('A) O forwarding não foi ativado corretamente — a bolha sugere que o valor de x5 deixou de ser encaminhado a tempo do estágio que faria a soma, algo resolvível pela configuração.'),
    q10.createChoice('B) O lw e o add formam um hazard WAW (Write-After-Write), pois as duas instruções passam pelo registrador x5 e o pipeline precisa serializar esse conflito de escrita.'),
    q10.createChoice('C) O branch predictor detectou um possível desvio de controle e inseriu a bolha preventivamente antes de confirmar que não havia nenhum branch relevante no trecho.'),
    q10.createChoice('D) O lw só entrega o valor de x5 ao final de MEM, mas o add precisa dele no início de EX. Mesmo com forwarding, o dado ainda não existe no ciclo necessário, então exatamente 1 stall continua inevitável no caso load-use.'),
    q10.createChoice('E) Não sei')
  ]);

  var q13 = form.addMultipleChoiceItem();
  q13.setTitle('Questão 8 — Stall vs. Flush: causas diferentes, efeito visual parecido\n\nEm um pipeline em execução, o aluno observa: (1) uma instrução permanece parada em estágios iniciais enquanto estágios à frente recebem bolhas; (2) várias instruções já avançadas no pipeline são descartadas e substituídas por bolhas. Por que esses dois eventos têm causas fundamentalmente diferentes?');
  q13.setRequired(true);
  q13.setChoices([
    q13.createChoice('A) Os dois eventos são a mesma coisa — nomes diferentes para qualquer ciclo improdutivo, sem distinção real entre causa, momento de detecção ou mecanismo interno.'),
    q13.createChoice('B) O primeiro ocorre apenas sem forwarding; o segundo só com forwarding ativado, já que os dois comportamentos são mutuamente exclusivos na lógica de controle do pipeline.'),
    q13.createChoice('C) O primeiro é um stall por hazard de dados: o pipeline segura uma instrução e injeta bolhas para esperar o operando. O segundo é um flush por hazard de controle: instruções já buscadas são descartadas porque vieram de um caminho errado, como após branch mal previsto ou exceção.'),
    q13.createChoice('D) O primeiro ocorre quando a D-Cache tem um miss; o segundo, quando a I-Cache tem um miss, então ambos são variações de latência do subsistema de memória.'),
    q13.createChoice('E) Não sei')
  ]);

  var q14 = form.addMultipleChoiceItem();
  q14.setTitle('Questão 9 — Identificar o tipo de hazard de dados em um trecho de código\n\nCom pipeline habilitado, o seguinte trecho gera uma dependência destacada:\n  mul x5, x1, x2  ← rd=x5\n  add x6, x5, x3  ← rs1=x5\nQual tipo de hazard de dados está sendo detectado, e por quê?');
  q14.setRequired(true);
  q14.setChoices([
    q14.createChoice('A) WAW (Write-After-Write): como ambas escrevem resultados em registradores, o pipeline detecta um conflito de escrita e força a serialização da confirmação final.'),
    q14.createChoice('B) WAR (Write-After-Read): o add lê x5 enquanto o mul ainda está produzindo esse valor, criando o risco de uma escrita posterior atrapalhar a leitura já iniciada.'),
    q14.createChoice('C) Nenhum hazard relevante: como os destinos finais são diferentes, a dependência observada é apenas aparente e não exige tratamento especial do hardware.'),
    q14.createChoice('D) RAW (Read-After-Write): o add precisa ler x5 como fonte antes de o mul terminar de escrevê-lo como destino. Em pipeline em ordem, essa dependência de dado é a que gera stall real e precisa de forwarding ou espera.'),
    q14.createChoice('E) Não sei')
  ]);

  var q15 = form.addMultipleChoiceItem();
  q15.setTitle('Questão 10 — Por que WAR e WAW não causam stalls em pipelines em-ordem\n\nAs estatísticas de execução de um pipeline em ordem mostram: hazards RAW aparecem dezenas de vezes; WAR e WAW aparecem zero vezes. O aluno suspeita de um bug. Por que essa distribuição é correta e esperada?');
  q15.setRequired(true);
  q15.setChoices([
    q15.createChoice('A) É um bug — em qualquer pipeline real, RAW, WAR e WAW tendem a aparecer com frequência semelhante sempre que várias instruções compartilham registradores.'),
    q15.createChoice('B) Em pipeline em ordem, as leituras acontecem antes das escritas posteriores e as instruções terminam na mesma sequência em que foram emitidas. Por isso WAR e WAW não viram stalls reais; quem gera espera de verdade é o RAW.'),
    q15.createChoice('C) WAR e WAW só aparecem em programas com laços muito longos ou rastros extensos; em exemplos curtos, é esperado que esses contadores permaneçam zerados.'),
    q15.createChoice('E) Não sei')
  ]);

  var q16 = form.addMultipleChoiceItem();
  q16.setTitle('Questão 11 — O que o CPI mede e o que ele não mede\n\nO resultado de uma simulação exibe "CPI: 1,85". Um aluno interpreta: "Cada instrução demorou 1,85 segundos para executar." O que essa leitura significa corretamente, e por que a interpretação está errada?');
  q16.setHelpText('Nesta questão, CPI significa Cycles Per Instruction (ciclos por instrução).');
  q16.setRequired(true);
  q16.setChoices([
    q16.createChoice('A) A interpretação está errada porque CPI é medido em nanosegundos, não em segundos; logo 1,85 já descreveria diretamente o tempo médio por instrução em hardware moderno.'),
    q16.createChoice('B) CPI é a média de ciclos de clock gastos por instrução ao longo da execução. Um CPI de 1,85 significa 1,85 ciclos por instrução em média; para virar tempo, ainda falta considerar o número de instruções e a frequência do clock.'),
    q16.createChoice('C) A interpretação está parcialmente correta: como ciclos modernos costumam ser muito curtos, o valor numérico do CPI pode ser lido aproximadamente como tempo em nanossegundos por instrução.'),
    q16.createChoice('D) CPI de 1,85 significa que o pipeline executou 1,85 instruções por ciclo, isto é, um IPC elevado apesar das bolhas e dos stalls observados.'),
    q16.createChoice('E) Não sei')
  ]);

  var q17 = form.addMultipleChoiceItem();
  q17.setTitle('Questão 12 — Por que o speedup do pipeline não é linear com o número de estágios\n\nUm aluno calcula que um pipeline de 5 estágios deveria oferecer speedup de 5×. Ao comparar execução pipelined vs. sequencial, obtém speedup real de apenas 2,8×. Qual combinação de fatores explica a diferença?');
  q17.setRequired(true);
  q17.setChoices([
    q17.createChoice('A) O speedup é limitado principalmente pelo barramento de memória: sem pipeline, a memória efetiva fica relativamente mais rápida e compensa boa parte da vantagem teórica do paralelismo entre estágios.'),
    q17.createChoice('B) O speedup real seria 5× se o programa não tivesse branches; como há desvios, eles se tornam o único fator relevante para derrubar o ganho abaixo do valor ideal.'),
    q17.createChoice('C) O speedup de 5× pressupõe que apenas instruções independentes sejam sobrepostas; sempre que surge qualquer dependência, o hardware volta ao comportamento quase sequencial e perde paralelismo.'),
    q17.createChoice('D) O speedup ideal de N× assume um pipeline perfeito e sem overhead. Na prática, stalls de dados, flushes de controle, misses de cache e desequilíbrio entre estágios somam ciclos extras, então o ganho observado fica bem abaixo do teto teórico.'),
    q17.createChoice('E) Não sei')
  ]);

  // ═══════════════════════════════════════════════════════════════════════════
  // SEÇÃO 3 — Atividade Guiada no RAVEN
  // ═══════════════════════════════════════════════════════════════════════════

  var pg3 = form.addPageBreakItem();
  pg3.setTitle('Seção 3 — Atividade Guiada no RAVEN (≈55 min)');
  pg3.setHelpText(
    'Sobre o RAVEN: é um simulador educacional de um processador RISC-V com visualização de pipeline, hierarquia de cache e execução multi-core.\n\n' +
    'Download: baixe o executável para a sua arquitetura em https://github.com/Gaok1/Raven-RiscV/releases/tag/v1.27.0 e execute o aplicativo.\n\n' +
    'Como funcionam as atividades: cada pergunta abaixo indica um preset identificado por um código (ex.: D1-01, D3-05). Na aba "Activity" do RAVEN, selecione o preset indicado — o simulador carrega automaticamente o programa e as configurações necessárias (pipeline, cache, multi-core).\n\n' +
    'Orientações gerais:\n' +
    '• Execute o preset até o final e anote os valores solicitados.\n' +
    '• Em campos numéricos, informe apenas o valor observado (use ponto ou vírgula para decimais).\n' +
    '• Em observações visuais, foque apenas no trecho ou momento indicado.\n' +
    '• As respostas descritivas valem pela observação, não pelo acerto.'
  );

  // ── D1: Pipeline ──────────────────────────────────────────────────────────

  form.addSectionHeaderItem().setTitle('D1 — Pipeline (≈12 min)');

  var d1p1a = form.addTextItem();
  d1p1a.setTitle('CPI (Cycles Per Instruction / ciclos por instrução) com instruções independentes');
  d1p1a.setHelpText(
    'Preset: D1-01 (carrega R100 + P100 + D101 automaticamente).\n\n' +
    'Execute o programa até o final e anote o CPI exibido na aba Run.\n' +
    'CPI = total de ciclos ÷ total de instruções executadas.'
  );
  d1p1a.setValidation(criarValidacaoNumeroDecimal('Informe apenas o CPI (Cycles Per Instruction / ciclos por instrução) observado no D101.'));
  d1p1a.setRequired(true);

  var d1p1b = form.addTextItem();
  d1p1b.setTitle('CPI com instruções dependentes (cada instrução usa o resultado da anterior)');
  d1p1b.setHelpText(
    'Preset: D1-02 (mantém R100 + D102, mas troca o pipeline para P102 com forwarding desativado).\n\n' +
    'Execute o programa e anote o CPI exibido na aba Run.\n' +
    'Compare com o valor obtido no D1-01.'
  );
  d1p1b.setValidation(criarValidacaoNumeroDecimal('Informe apenas o CPI (Cycles Per Instruction / ciclos por instrução) observado no D102.'));
  d1p1b.setRequired(true);

  var d1p2 = form.addParagraphTextItem();
  d1p2.setTitle('Throughput vs. latência individual — o que o pipeline realmente melhora?');
  d1p2.setHelpText(
    'Use qualquer um dos programas acima com as mesmas configurações.\n' +
    'Abra a aba Pipeline e avance poucos ciclos — o suficiente para aparecerem várias instruções simultâneas no diagrama.\n\n' +
    'Observe o diagrama e descreva o que você nota sobre como as instruções se movem pelos estágios.\n' +
    'O que mudou em relação à execução sem pipeline?'
  );
  d1p2.setRequired(true);

  // ── D2: Hazards ───────────────────────────────────────────────────────────

  form.addSectionHeaderItem().setTitle('D2 — Hazards de Dados e Controle (≈15 min)');

  var d2p1a = form.addTextItem();
  d2p1a.setTitle('Load-Use Hazard — bolhas automáticas no par lw/add');
  d2p1a.setHelpText(
    'Preset: D2-01 (carrega R100 + P100 + D201 automaticamente).\n\n' +
    'Avance ciclo a ciclo na aba Pipeline até a primeira ocorrência do par lw/add no diagrama.\n' +
    'Bolhas são ciclos vazios inseridos pelo pipeline entre instruções — aparecem como células em branco.\n\n' +
    'Quantas bolhas aparecem antes do add nessa primeira ocorrência? (0 = nenhuma)'
  );
  d2p1a.setValidation(criarValidacaoInteiro(0, 99, 'Informe apenas a quantidade de bolhas observada.'));
  d2p1a.setRequired(true);

  var d2p1b = form.addTextItem();
  d2p1b.setTitle('Load-Use Hazard — efeito de inserir um nop entre lw e add');
  d2p1b.setHelpText(
    'Mantenha as mesmas configurações (Preset D2-01 já carregado).\n\n' +
    'No Editor, localize as linhas com lw e add (D201.fas) e insira uma linha "nop" entre elas.\n' +
    'Pressione F5 para recompilar. Execute novamente e observe o par lw/nop/add no diagrama.\n\n' +
    'Quantas bolhas aparecem antes do add agora? Compare com o resultado anterior.'
  );
  d2p1b.setValidation(criarValidacaoInteiro(0, 99, 'Informe apenas a quantidade de bolhas observada após inserir o nop.'));
  d2p1b.setRequired(true);

  var d2p2 = form.addParagraphTextItem();
  d2p2.setTitle('Hazards WAR e WAW: por que aparecem zero no RAVEN?');
  d2p2.setHelpText(
    'Com o programa executado, observe os contadores de hazards na aba Pipeline.\n\n' +
    'RAW (Read-After-Write), WAR (Write-After-Read) e WAW (Write-After-Write)\n' +
    'são três tipos de dependência entre instruções que podem causar problemas no pipeline.\n\n' +
    'Note o que aparece e o que permanece em zero. O que isso pode indicar sobre a ordem\n' +
    'em que as instruções avançam pelos estágios no RAVEN?'
  );
  d2p2.setRequired(true);

  var d2p3 = form.addParagraphTextItem();
  d2p3.setTitle('Diferença visual entre stall e flush no diagrama de pipeline');
  d2p3.setHelpText(
    'Preset: D2-01 para o primeiro evento / D2-02 para o segundo (mantém R100 + P100, troca o programa).\n\n' +
    'No D201: avance até o primeiro par lw/add e observe o que acontece com as instruções ao redor.\n' +
    'No D202: avance até o primeiro salto (jal) e observe o que acontece logo após.\n\n' +
    'Como os dois momentos se parecem visualmente no diagrama?\n' +
    'O que diferencia o comportamento das instruções em cada caso?'
  );
  d2p3.setRequired(true);

  // ── D3: Cache ─────────────────────────────────────────────────────────────

  form.addSectionHeaderItem().setTitle('D3 — Cache (≈10 min)');

  // D3 — Experimento 1: Tamanho vs. Latência de Acesso (AMAT)

  var d3p1a = form.addTextItem();
  d3p1a.setTitle('AMAT (Average Memory Access Time) — Config A: cache pequena, hit time baixo');
  d3p1a.setHelpText(
    'Preset: D3-01 (carrega R300 + P101 + C311 + D301b automaticamente).\n\n' +
    'Config A: D-Cache 256 B, 2-way, hit time = 1 ciclo.\n\n' +
    'Execute o programa e leia o AMAT da D-Cache no resumo da aba Cache.\n' +
    'AMAT = Hit Time + Miss Rate × Miss Penalty.\n\n' +
    'Informe o valor observado (em ciclos).'
  );
  d3p1a.setValidation(criarValidacaoNumeroDecimal('Informe apenas o AMAT da Config A em ciclos.'));
  d3p1a.setRequired(true);

  var d3p1b = form.addTextItem();
  d3p1b.setTitle('AMAT — Config B: cache maior, hit time mais alto');
  d3p1b.setHelpText(
    'Preset: D3-02 (mantém R300 + P101 + D301b, troca para C312).\n\n' +
    'Config B: D-Cache 1 KB, 2-way, hit time = 4 ciclos.\n\n' +
    'Execute e leia o AMAT da D-Cache na aba Cache.\n\n' +
    'Informe o valor observado e compare com o da Config A.'
  );
  d3p1b.setValidation(criarValidacaoNumeroDecimal('Informe apenas o AMAT da Config B em ciclos.'));
  d3p1b.setRequired(true);

  var d3p1c = form.addMultipleChoiceItem();
  d3p1c.setTitle('Com base no AMAT observado, qual configuração apresentou melhor desempenho de acesso à memória?');
  d3p1c.setHelpText(
    'Use os dois valores que você mediu acima.'
  );
  d3p1c.setRequired(true);
  d3p1c.setChoices([
    d3p1c.createChoice('Config A (256 B, hit time 1 ciclo)'),
    d3p1c.createChoice('Config B (1 KB, hit time 4 ciclos)'),
    d3p1c.createChoice('Empate / muito semelhante')
  ]);

  // D3 — Experimento 2: Política de Substituição em Varredura Sequencial

  var d3p2a = form.addTextItem();
  d3p2a.setTitle('Miss rate com política LRU (Least Recently Used) em varredura sequencial');
  d3p2a.setHelpText(
    'Preset: D3-03 (carrega R300 + P101 + C321 + D301).\n\n' +
    'Execute e leia o D-Cache miss rate no resumo da aba Cache.\n' +
    'LRU = descarta o bloco acessado há mais tempo.\n\n' +
    'Informe o miss rate observado (entre 0 e 1, ou porcentagem).'
  );
  d3p2a.setValidation(criarValidacaoNumeroDecimal('Informe apenas o miss rate observado com LRU.'));
  d3p2a.setRequired(true);

  var d3p2b = form.addTextItem();
  d3p2b.setTitle('Miss rate com política FIFO (First In, First Out) em varredura sequencial');
  d3p2b.setHelpText(
    'Preset: D3-04 (mantém R300 + P101 + D301, troca para C322).\n\n' +
    'Execute e leia o D-Cache miss rate na aba Cache.\n' +
    'FIFO = descarta o bloco que entrou na cache há mais tempo.\n\n' +
    'Informe o miss rate e compare com o resultado do LRU acima.'
  );
  d3p2b.setValidation(criarValidacaoNumeroDecimal('Informe apenas o miss rate observado com FIFO.'));
  d3p2b.setRequired(true);

  var d3p2c = form.addParagraphTextItem();
  d3p2c.setTitle('LRU vs. FIFO em varredura sequencial — por que o resultado é (quase) o mesmo?');
  d3p2c.setHelpText(
    'Compare os miss rates de LRU (C321) e FIFO (C322) com D301.\n\n' +
    'O que os dois valores sugerem sobre como o D301 acessa a memória?\n' +
    'Em que tipo de situação você esperaria que LRU e FIFO produzissem resultados diferentes?'
  );
  d3p2c.setRequired(true);

  // ── D4: ISA RISC-V ────────────────────────────────────────────────────────

  form.addSectionHeaderItem().setTitle('D4 — ISA (Instruction Set Architecture) RISC-V (≈8 min)');

  var d4p1 = form.addParagraphTextItem();
  d4p1.setTitle('Codificação binária: por que S-type não tem campo rd?');
  d4p1.setHelpText(
    'Preset: D4-01 (carrega R100 + P101 + D401 automaticamente).\n\n' +
    'Na aba Run, mova o cursor sobre a instrução add e depois sobre a instrução sw.\n' +
    'O painel de detalhes à direita mostra a codificação binária de cada uma.\n\n' +
    'Compare os bits [11:7] nas duas instruções. O que você encontra em cada caso?\n' +
    'O que isso sugere sobre a diferença de propósito entre as duas instruções?'
  );
  d4p1.setRequired(true);

  // ── D5: Hardware Threads ──────────────────────────────────────────────────

  form.addSectionHeaderItem().setTitle('D5 — Hardware Threads / Multi-Core (≈8 min)');

  var d5p1 = form.addMultipleChoiceItem();
  d5p1.setTitle('Com dois cores executando programas diferentes, ao inspecionar seus bancos de registradores, os valores de x1, x5 e o PC são:');
  d5p1.setHelpText(
    'Preset: D5-01 (carrega R500 + P101 + D501 com 2 cores configurados).\n\n' +
    'Execute o programa por alguns ciclos.\n' +
    'Na aba Run, troque entre Core 0 e Core 1 usando o seletor de core.\n' +
    'Observe x1, x5 e o PC (Program Counter) em cada core.'
  );
  d5p1.setRequired(true);
  d5p1.setChoices([
    d5p1.createChoice('Idênticos nos dois cores, pois compartilham o mesmo banco de registradores'),
    d5p1.createChoice('Diferentes, pois cada core possui seu próprio banco de registradores privado, completamente independente do outro'),
    d5p1.createChoice('Idênticos para registradores de dados (x1–x31), mas com PC diferente em cada core'),
    d5p1.createChoice('Diferentes apenas enquanto os programas executam; sincronizam ao final da execução')
  ]);

  var d5p2 = form.addParagraphTextItem();
  d5p2.setTitle('O que Core 0 e Core 1 compartilham, e o que é privado em cada um?');
  d5p2.setHelpText(
    'Com os dois cores executando, compare o que você vê ao trocar entre eles:\n' +
    '• Os registradores mudam ao trocar de core?\n' +
    '• A visão da RAM muda?\n\n' +
    'Descreva o que parece ser individual de cada core e o que parece ser comum aos dois.'
  );
  d5p2.setRequired(true);

  // ── D6: Métricas ──────────────────────────────────────────────────────────

  form.addSectionHeaderItem().setTitle('D6 — Métricas de Desempenho (≈7 min)');

  var d6p1a = form.addTextItem();
  d6p1a.setTitle('CPI (Cycles Per Instruction) sem pipeline — execução sequencial');
  d6p1a.setHelpText(
    'Preset: D6-01 (carrega R100 + P101 + D102 — pipeline desabilitado).\n\n' +
    'Execute o programa e anote o CPI exibido na aba Run.'
  );
  d6p1a.setValidation(criarValidacaoNumeroDecimal('Informe apenas o CPI observado sem pipeline.'));
  d6p1a.setRequired(true);

  var d6p1b = form.addTextItem();
  d6p1b.setTitle('CPI com pipeline habilitado — execução sobreposta');
  d6p1b.setHelpText(
    'Preset: D6-02 (mantém R100 + D102, habilita pipeline com P100).\n\n' +
    'Execute o mesmo programa e anote o CPI.'
  );
  d6p1b.setValidation(criarValidacaoNumeroDecimal('Informe apenas o CPI observado com pipeline.'));
  d6p1b.setRequired(true);

  var d6p1c = form.addTextItem();
  d6p1c.setTitle('Speedup observado (CPI sem pipeline ÷ CPI com pipeline)');
  d6p1c.setHelpText(
    'Speedup = CPI_sem ÷ CPI_com.\n\n' +
    'Calcule com os dois valores acima e informe o resultado.'
  );
  d6p1c.setValidation(criarValidacaoNumeroDecimal('Informe apenas o speedup calculado.'));
  d6p1c.setRequired(true);

  var d6p2 = form.addParagraphTextItem();
  d6p2.setTitle('Por que o speedup ficou abaixo do esperado?');
  d6p2.setHelpText(
    'Com base no que você observou ao longo da sessão:\n' +
    'o que pode ter impedido o pipeline de atingir o speedup máximo possível?\n\n' +
    'Justifique com base no que você viu no RAVEN.'
  );
  d6p2.setRequired(true);

  // ═══════════════════════════════════════════════════════════════════════════
  // SEÇÃO 4 — Pós-Teste
  // ═══════════════════════════════════════════════════════════════════════════

  var pg4 = form.addPageBreakItem();
  pg4.setTitle('Seção 4 — Pós-Teste');
  pg4.setHelpText(
    'Responda com base no que foi trabalhado durante a sessão com o RAVEN. ' +
    'Não consulte materiais externos. As questões são as mesmas da seção inicial — ' +
    'responda de forma independente. Tempo estimado: 15 minutos.'
  );

  var pos1 = form.addMultipleChoiceItem();
  pos1.setTitle('Questão 1 — Formatos de instrução RISC-V\n\nQuais formatos de instrução RISC-V não possuem o campo rd (registrador de destino) em sua codificação binária?');
  pos1.setRequired(true);
  pos1.setChoices([
    pos1.createChoice('A) R-type (add, sub, mul) e I-type (addi, lw, jalr) — porque usam dois operandos fonte e poderiam reaproveitar o espaço de rd para ampliar o imediato.'),
    pos1.createChoice('B) S-type (sw, sb) e B-type (beq, bne) — porque produzem efeito em memória ou no PC e não escrevem em registrador de destino. Nesses formatos, os bits [11:7] são reaproveitados para codificar parte do imediato.'),
    pos1.createChoice('C) U-type (upper immediate) e J-type (jump) — porque o imediato de 20 bits ocupa quase toda a instrução e empurra o campo rd para fora da codificação útil.'),
    pos1.createChoice('D) Apenas ECALL e EBREAK — porque instruções de sistema são tratadas por um caminho especial de controle e, por isso, dispensam qualquer registrador de destino.')
  ]);

  var pos4 = form.addMultipleChoiceItem();
  pos4.setTitle('Questão 2 — Função do campo tag no endereçamento de cache\n\nUm endereço de memória é dividido em: offset, índice e tag. Qual desses campos verifica se o bloco armazenado em um set corresponde ao endereço sendo acessado?');
  pos4.setRequired(true);
  pos4.setChoices([
    pos4.createChoice('A) O campo índice, porque identifica unicamente o set e — em mapeamento direto — também identifica unicamente o bloco armazenado naquele set.'),
    pos4.createChoice('B) O campo offset, porque indica a posição exata dentro do bloco.'),
    pos4.createChoice('C) O campo tag, porque múltiplos endereços distintos mapeiam para o mesmo set. A tag carrega os bits de alta ordem e permite confirmar se o bloco presente naquele set pertence ao endereço sendo acessado.'),
    pos4.createChoice('D) Nenhum campo adicional é necessário em mapeamento direto — o índice já garante que o bloco é o correto.')
  ]);

  var pos5 = form.addMultipleChoiceItem();
  pos5.setTitle('Questão 3 — Limitação da política LRU (Least Recently Used) em varredura sequencial\n\nPor que a política LRU falha para programas que fazem varreduras sequenciais de arrays grandes (padrão streaming)?');
  pos5.setHelpText('Nesta questão, FIFO significa First In, First Out.');
  pos5.setRequired(true);
  pos5.setChoices([
    pos5.createChoice('A) Falha porque o LRU exige mais hardware que FIFO (First In, First Out) — esse overhead de manutenção do rank pode anular o ganho esperado em hit rate quando o array é muito grande.'),
    pos5.createChoice('B) Falha porque em streaming o programa percorre os blocos em ordem e quase não volta a usá-los no curto prazo. Sem localidade temporal, o LRU preserva linhas que já cumpriram seu papel e não serão reutilizadas tão cedo.'),
    pos5.createChoice('C) Falha apenas quando o array não cabe inteiramente na cache — se couber, LRU passa a ser automaticamente a melhor política para qualquer padrão de acesso sequencial.'),
    pos5.createChoice('D) Falha porque varreduras sequenciais ativam um modo de prefetch automático que contorna a política LRU, tornando a escolha de política praticamente irrelevante.')
  ]);

  var pos6 = form.addMultipleChoiceItem();
  pos6.setTitle('Questão 4 — AMAT (Average Memory Access Time / tempo médio de acesso à memória) e o trade-off entre tamanho e latência de cache\n\n• Config A: 16 KB, 4-way, hit time = 1 ciclo, miss rate = 8%\n• Config B: 64 KB, 4-way, hit time = 4 ciclos, miss rate = 6%\nAmbas com miss penalty = 50 ciclos. Como o AMAT da Config B pode ser maior mesmo com miss rate menor?');
  pos6.setRequired(true);
  pos6.setChoices([
    pos6.createChoice('A) AMAT = Hit Time + Miss Rate × Miss Penalty. A Config B sofre menos misses, mas cada hit custa muito mais; se esse aumento de hit time superar o ganho na miss rate, o AMAT total piora mesmo em uma cache maior.'),
    pos6.createChoice('B) É um erro de configuração — hit time acima de 1 ciclo não deveria aparecer em L1, então a diferença observada indica parâmetros incoerentes ou entrada incorreta no experimento.'),
    pos6.createChoice('C) Indica que a miss penalty domina completamente — por isso diferenças de hit time quase não pesam no AMAT final quando o programa acessa memória com frequência.'),
    pos6.createChoice('D) Caches maiores sempre produzem AMAT menor, independentemente do hit time — a fórmula AMAT = Hit Time + Miss Rate × Miss Penalty não captura esse efeito, portanto a comparação entre Config A e B por AMAT não tem validade.')
  ]);

  var pos8 = form.addMultipleChoiceItem();
  pos8.setTitle('Questão 5 — Função do estágio WB no pipeline de cinco estágios\n\nNo pipeline IF → ID → EX → MEM → WB. Qual é a função primária do estágio WB?');
  pos8.setRequired(true);
  pos8.setChoices([
    pos8.createChoice('A) Ler os operandos do banco de registradores (rs1, rs2) e detectar dependências de dados entre instruções em voo.'),
    pos8.createChoice('B) Escrever o resultado da operação no registrador de destino (rd) especificado pela instrução.'),
    pos8.createChoice('C) Buscar a próxima instrução na memória de instruções usando o valor atual do PC.'),
    pos8.createChoice('D) Executar a operação aritmética ou lógica na ULA sobre os operandos preparados no estágio anterior.')
  ]);

  var pos9 = form.addMultipleChoiceItem();
  pos9.setTitle('Questão 6 — Pipeline aumenta throughput, não reduz latência individual\n\nPor que a afirmação "o pipeline acelera o processamento porque cada instrução individual é executada mais rápido" está incorreta?');
  pos9.setRequired(true);
  pos9.setChoices([
    pos9.createChoice('A) Está incorreta porque o pipeline reduz a frequência de clock para acomodar os múltiplos estágios, e o ganho vem apenas do paralelismo entre programas diferentes.'),
    pos9.createChoice('B) Está incorreta porque o ganho vem exclusivamente do cache de instruções (I-Cache) — o pipeline em si não contribui.'),
    pos9.createChoice('C) Está incorreta porque o pipeline aumenta o throughput (instruções finalizadas por unidade de tempo) ao sobrepor execução de múltiplas instruções em estágios distintos, mas a latência individual de cada instrução permanece a mesma ou até aumenta levemente.'),
    pos9.createChoice('D) Está incorreta porque o pipeline só acelera instruções aritméticas; instruções de memória e branches têm a mesma latência em execução pipelined e sequencial.')
  ]);

  var pos10 = form.addMultipleChoiceItem();
  pos10.setTitle('Questão 7 — Load-Use Hazard com forwarding ativo\n\n  lw  x5, 0(x1)\n  add x6, x5, x2\n\nCom pipeline e forwarding ativos, por que uma bolha ainda é inserida?');
  pos10.setHelpText('Nesta questão, WAW significa Write-After-Write.');
  pos10.setRequired(true);
  pos10.setChoices([
    pos10.createChoice('A) O forwarding não foi ativado corretamente — a bolha sugere que o valor de x5 deixou de ser encaminhado a tempo do estágio que faria a soma, algo resolvível pela configuração.'),
    pos10.createChoice('B) Formam um hazard WAW (Write-After-Write), pois as duas instruções passam pelo registrador x5 e o pipeline precisa serializar esse conflito de escrita.'),
    pos10.createChoice('C) O branch predictor detectou um possível desvio e inseriu a bolha preventivamente antes de confirmar que não havia nenhum branch relevante no trecho.'),
    pos10.createChoice('D) O lw só entrega o valor de x5 ao final de MEM, mas o add precisa dele no início de EX. Mesmo com forwarding, o dado ainda não existe no ciclo necessário, então exatamente 1 stall continua inevitável no caso load-use.')
  ]);

  var pos13 = form.addMultipleChoiceItem();
  pos13.setTitle('Questão 8 — Stall vs. Flush: causas diferentes, efeito visual parecido\n\nPor que (1) instrução parada com bolhas à frente e (2) várias instruções avançadas sendo descartadas têm causas fundamentalmente diferentes?');
  pos13.setRequired(true);
  pos13.setChoices([
    pos13.createChoice('A) Os dois eventos são a mesma coisa — nomes diferentes para qualquer ciclo improdutivo, sem distinção real entre causa, momento de detecção ou mecanismo interno.'),
    pos13.createChoice('B) O primeiro ocorre apenas sem forwarding; o segundo só com forwarding ativado, já que os dois comportamentos são mutuamente exclusivos na lógica de controle do pipeline.'),
    pos13.createChoice('C) O primeiro é um stall por hazard de dados: o pipeline segura uma instrução e injeta bolhas para esperar o operando. O segundo é um flush por hazard de controle: instruções já buscadas são descartadas porque vieram de um caminho errado, como após branch mal previsto ou exceção.'),
    pos13.createChoice('D) O primeiro ocorre quando a D-Cache tem um miss; o segundo, quando a I-Cache tem um miss, então ambos são variações de latência do subsistema de memória.')
  ]);

  var pos14 = form.addMultipleChoiceItem();
  pos14.setTitle('Questão 9 — Identificar o tipo de hazard de dados em um trecho de código\n\n  mul x5, x1, x2  ← rd=x5\n  add x6, x5, x3  ← rs1=x5\n\nQual tipo de hazard de dados está sendo detectado, e por quê?');
  pos14.setRequired(true);
  pos14.setChoices([
    pos14.createChoice('A) WAW (Write-After-Write): como ambas escrevem resultados em registradores, o pipeline detecta um conflito de escrita e força a serialização da confirmação final.'),
    pos14.createChoice('B) WAR (Write-After-Read): o add lê x5 enquanto o mul ainda está produzindo esse valor, criando o risco de uma escrita posterior atrapalhar a leitura já iniciada.'),
    pos14.createChoice('C) Nenhum hazard relevante: como os destinos finais são diferentes, a dependência observada é apenas aparente e não exige tratamento especial do hardware.'),
    pos14.createChoice('D) RAW (Read-After-Write): o add precisa ler x5 como fonte antes de o mul terminar de escrevê-lo como destino. Em pipeline em ordem, essa dependência de dado é a que gera stall real e precisa de forwarding ou espera.')
  ]);

  var pos15 = form.addMultipleChoiceItem();
  pos15.setTitle('Questão 10 — Por que WAR e WAW não causam stalls em pipelines em-ordem\n\nAs estatísticas mostram: hazards RAW aparecem dezenas de vezes; WAR e WAW aparecem zero vezes. Por que essa distribuição é correta e esperada?');
  pos15.setRequired(true);
  pos15.setChoices([
    pos15.createChoice('A) É um bug — em qualquer pipeline real, RAW, WAR e WAW tendem a aparecer com frequência semelhante sempre que várias instruções compartilham registradores.'),
    pos15.createChoice('B) Em pipeline em ordem, as leituras acontecem antes das escritas posteriores e as instruções terminam na mesma sequência em que foram emitidas. Por isso WAR e WAW não viram stalls reais; quem gera espera de verdade é o RAW.'),
    pos15.createChoice('C) WAR e WAW só aparecem em programas com laços muito longos ou rastros extensos; em exemplos curtos, é esperado que esses contadores permaneçam zerados.')
  ]);

  var pos16 = form.addMultipleChoiceItem();
  pos16.setTitle('Questão 11 — O que o CPI mede e o que ele não mede\n\nO painel exibe "CPI: 1,85". Por que a interpretação "cada instrução demorou 1,85 segundos" está errada?');
  pos16.setHelpText('Nesta questão, CPI significa Cycles Per Instruction (ciclos por instrução).');
  pos16.setRequired(true);
  pos16.setChoices([
    pos16.createChoice('A) CPI é medido em nanosegundos, não em segundos; logo 1,85 já descreveria diretamente o tempo médio por instrução em hardware moderno.'),
    pos16.createChoice('B) CPI é a média de ciclos de clock gastos por instrução ao longo da execução. Um CPI de 1,85 significa 1,85 ciclos por instrução em média; para virar tempo, ainda falta considerar o número de instruções e a frequência do clock.'),
    pos16.createChoice('C) A interpretação está parcialmente correta: como ciclos modernos costumam ser muito curtos, o valor numérico do CPI pode ser lido aproximadamente como tempo em nanossegundos por instrução.'),
    pos16.createChoice('D) CPI de 1,85 significa que o pipeline executou 1,85 instruções por ciclo, isto é, um IPC elevado apesar das bolhas e dos stalls observados.')
  ]);

  var pos17 = form.addMultipleChoiceItem();
  pos17.setTitle('Questão 12 — Por que o speedup do pipeline não é linear\n\nUm pipeline de 5 estágios produz speedup real de apenas 2,8× em vez de 5×. Qual combinação de fatores explica a diferença?');
  pos17.setRequired(true);
  pos17.setChoices([
    pos17.createChoice('A) O speedup é limitado principalmente pelo barramento de memória: sem pipeline, a memória efetiva fica relativamente mais rápida e compensa boa parte da vantagem teórica do paralelismo entre estágios.'),
    pos17.createChoice('B) O speedup real seria 5× se o programa não tivesse branches; como há desvios, eles se tornam o único fator relevante para derrubar o ganho abaixo do valor ideal.'),
    pos17.createChoice('C) O speedup de 5× pressupõe que apenas instruções independentes sejam sobrepostas; sempre que surge qualquer dependência, o hardware volta ao comportamento quase sequencial e perde paralelismo.'),
    pos17.createChoice('D) O speedup ideal de N× assume um pipeline perfeito e sem overhead. Na prática, stalls de dados, flushes de controle, misses de cache e desequilíbrio entre estágios somam ciclos extras, então o ganho observado fica bem abaixo do teto teórico.')
  ]);

  // ─── Log final ─────────────────────────────────────────────────────────────
  Logger.log('✅ Formulário unificado criado com sucesso!');
  Logger.log('🔗 Link para edição: ' + form.getEditUrl());
  Logger.log('📋 Link para participantes: ' + form.getPublishedUrl());
}
