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
 *   Seção 2 — Pré-Teste (Q1–Q17 com "Não sei")
 *   Seção 3 — Atividade Guiada no RAVEN (D1–D6, ~65 min)
 *   Seção 4 — Pós-Teste (Q1–Q17 sem "Não sei")
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
    'A sessão tem duração aproximada de 105 a 115 minutos e está organizada em quatro partes: ' +
    'perfil do participante, questionário inicial, atividade prática com o simulador e questionário final. ' +
    'As respostas são anônimas e utilizadas exclusivamente para fins de pesquisa.'
  );
  form.setCollectEmail(false);
  form.setShowLinkToRespondAgain(false);

  // ═══════════════════════════════════════════════════════════════════════════
  // SEÇÃO 1 — Perfil do Participante
  // ═══════════════════════════════════════════════════════════════════════════

  form.addSectionHeaderItem().setTitle('Seção 1 — Perfil do Participante');

  // Forma de identificação
  var p1a = form.addMultipleChoiceItem();
  p1a.setTitle('Como você prefere se identificar nesta pesquisa?');
  p1a.setRequired(true);
  p1a.setChoices([
    p1a.createChoice('Número de matrícula'),
    p1a.createChoice('E-mail pessoal')
  ]);

  // Identificador
  var p1b = form.addTextItem();
  p1b.setTitle('Identificador');
  p1b.setHelpText(
    'De acordo com a opção escolhida acima:\n' +
    '• Número de matrícula: informe o número completo\n' +
    '• E-mail pessoal: informe seu e-mail'
  );
  p1b.setValidation(
    criarValidacaoPorRegex(
      '^(?:\\d{4,20}|[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,})$',
      'Informe apenas um número de matrícula (somente dígitos) ou um e-mail válido.'
    )
  );
  p1b.setRequired(true);

  // Curso
  var p2 = form.addMultipleChoiceItem();
  p2.setTitle('Qual é o seu curso?');
  p2.setRequired(true);
  p2.setChoices([
    p2.createChoice('Ciência da Computação'),
    p2.createChoice('Engenharia de Computação'),
    p2.createChoice('Sistemas de Informação'),
    p2.createChoice('Outro')
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
  p4b.setHelpText(
    'Marque todos os que se aplicam, independentemente da disciplina. ' +
    'Siglas usadas nesta lista: ISA = Instruction Set Architecture; RAW = Read-After-Write; WAR = Write-After-Read; ' +
    'WAW = Write-After-Write; LRU = Least Recently Used; FIFO = First In, First Out; ' +
    'AMAT = Average Memory Access Time; CPI = Cycles Per Instruction.'
  );
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
  p5.showOtherOption(true);

  // ═══════════════════════════════════════════════════════════════════════════
  // SEÇÃO 2 — Pré-Teste
  // ═══════════════════════════════════════════════════════════════════════════

  var pg2 = form.addPageBreakItem();
  pg2.setTitle('Seção 2 — Pré-Teste');
  pg2.setHelpText(
    'Responda com base no seu conhecimento atual, sem consultar materiais ou o simulador. ' +
    'As questões avaliam compreensão de conceitos, não realização de cálculos. ' +
    'A opção "E) Não sei" está disponível e deve ser usada quando não houver certeza — ' +
    'ela é parte do instrumento de avaliação. Tempo estimado: 20 minutos.'
  );

  var q1 = form.addMultipleChoiceItem();
  q1.setTitle('Questão 1 — Formatos de instrução RISC-V\n\nQuais formatos de instrução RISC-V não possuem o campo rd (registrador de destino) em sua codificação binária?');
  q1.setRequired(true);
  q1.setChoices([
    q1.createChoice('A) R-type (add, sub, mul) e I-type (addi, lw, jalr) — porque operações com dois registradores de fonte nunca precisam de destino explícito na codificação binária.'),
    q1.createChoice('B) S-type (sw, sb) e B-type (beq, bne) — porque produzem efeito em memória (store) ou no PC (branch) e não escrevem em nenhum registrador de destino. Os bits [11:7] codificam parte do imediato nesses formatos.'),
    q1.createChoice('C) U-type (upper immediate) e J-type (jump) — porque trabalham com imediatos de 20 bits que ocupam todo o espaço disponível, inclusive os bits onde normalmente estaria o rd.'),
    q1.createChoice('D) Apenas ECALL e EBREAK — por serem instruções de sistema tratadas de forma especial pelo hardware, sem registrador de destino definido.'),
    q1.createChoice('E) Não sei')
  ]);

  var q2 = form.addMultipleChoiceItem();
  q2.setTitle('Questão 2 — Desempenho RISC vs. CISC\n\nUm aluno conclui: "Processadores RISC precisam de mais instruções para realizar o mesmo trabalho e, portanto, são mais lentos que CISC." Por que esse argumento é uma simplificação incorreta?');
  q2.setHelpText('Nesta questão, CPI significa Cycles Per Instruction (ciclos por instrução).');
  q2.setRequired(true);
  q2.setChoices([
    q2.createChoice('A) O argumento está correto — arquiteturas RISC executam mais instruções que CISC, mas compensam com frequências de clock muito mais altas que dobram o desempenho de forma consistente.'),
    q2.createChoice('B) Está incorreto porque o CPI de processadores RISC é sempre exatamente 1 — como cada instrução completa em 1 ciclo, o número maior de instruções se compensa automaticamente.'),
    q2.createChoice('C) Está incorreto porque o número de instruções por programa não é um bom indicador de desempenho isoladamente — o que importa é o tempo total (N × CPI × período de clock). A regularidade RISC viabiliza pipelines mais profundos e frequências mais altas, o que frequentemente compensa o overhead em contagem de instruções.'),
    q2.createChoice('D) Está incorreto apenas para operações de ponto flutuante — para operações inteiras puras, arquiteturas CISC de fato superam RISC em desempenho absoluto por instrução.'),
    q2.createChoice('E) Não sei')
  ]);

  var q3 = form.addMultipleChoiceItem();
  q3.setTitle('Questão 3 — Contexto de execução em multi-core\n\nDois cores de um processador executam programas em paralelo. Ao inspecionar o banco de registradores do Core 0 e do Core 1, os valores são completamente diferentes — x1, x5, x10 e até o PC apontam para valores distintos em cada core. Por que isso é correto e esperado?');
  q3.setHelpText('Nesta questão, PC significa Program Counter (contador de programa).');
  q3.setRequired(true);
  q3.setChoices([
    q3.createChoice('A) Os dois cores compartilham o mesmo banco de registradores físico — o que difere é apenas o PC, que aponta para posições distintas de um único fluxo de instruções compartilhado.'),
    q3.createChoice('B) As diferenças são temporárias — os bancos sincronizam automaticamente via cache coherence ao fim de cada fase de execução.'),
    q3.createChoice('C) Os registradores diferem porque os dois cores operam em frequências de clock distintas; em hardware com clock unificado, os valores seriam idênticos.'),
    q3.createChoice('D) Cada core possui seu próprio banco de registradores privado (x0–x31), seu próprio PC e estado interno completamente independentes. O que compartilham é apenas a memória principal (RAM) e o subsistema de cache.'),
    q3.createChoice('E) Não sei')
  ]);

  var q4 = form.addMultipleChoiceItem();
  q4.setTitle('Questão 4 — Função do campo tag no endereçamento de cache\n\nUm endereço de memória é dividido em: offset (seleciona o byte dentro do bloco), índice (seleciona o set da cache) e tag (bits de alta ordem). Qual desses campos é usado para verificar se o bloco armazenado em um determinado set corresponde ao endereço sendo acessado?');
  q4.setRequired(true);
  q4.setChoices([
    q4.createChoice('A) O campo índice, porque ele identifica unicamente o set e — em mapeamento direto — também identifica unicamente o bloco armazenado naquele set.'),
    q4.createChoice('B) O campo offset, porque indica a posição exata dentro do bloco, garantindo que os bytes corretos sejam retornados ao processador.'),
    q4.createChoice('C) O campo tag, porque múltiplos endereços distintos mapeiam para o mesmo set. A tag carrega os bits de alta ordem e permite confirmar se o bloco presente naquele set pertence ao endereço sendo acessado.'),
    q4.createChoice('D) Nenhum campo adicional é necessário em mapeamento direto — o índice já garante que o bloco é o correto, pois cada set só pode conter dados de um único endereço por vez.'),
    q4.createChoice('E) Não sei')
  ]);

  var q5 = form.addMultipleChoiceItem();
  q5.setTitle('Questão 5 — Limitação da política LRU em varredura sequencial\n\nUm aluno argumenta que LRU (Least Recently Used) é sempre a melhor escolha porque "o que foi usado mais recentemente provavelmente será usado de novo." Por que essa lógica falha para programas que fazem varreduras sequenciais de arrays grandes (padrão streaming)?');
  q5.setHelpText('Nesta questão, FIFO significa First In, First Out.');
  q5.setRequired(true);
  q5.setChoices([
    q5.createChoice('A) Falha porque o LRU exige mais hardware que FIFO (First In, First Out) — o overhead de manutenção do rank aumenta o hit time a ponto de superar o ganho em hit rate para arrays grandes.'),
    q5.createChoice('B) Falha porque em streaming o programa acessa cada elemento exatamente uma vez, em ordem, sem retornar ao mesmo bloco no curto prazo. A localidade temporal simplesmente não existe em streaming — LRU mantém blocos que não serão reutilizados.'),
    q5.createChoice('C) Falha apenas quando o array não cabe inteiramente na cache — se couber, LRU tem desempenho perfeito para qualquer padrão de acesso, incluindo sequencial.'),
    q5.createChoice('D) Falha porque varreduras sequenciais ativam um modo de prefetch automático no hardware que contorna a política LRU, tornando a escolha de política irrelevante.'),
    q5.createChoice('E) Não sei')
  ]);

  var q6 = form.addMultipleChoiceItem();
  q6.setTitle('Questão 6 — AMAT (Average Memory Access Time / tempo médio de acesso à memória) e o trade-off entre tamanho e latência de cache\n\nDois projetos de cache L1:\n• Config A: 16 KB, 4-way, hit time = 1 ciclo, miss rate = 8%\n• Config B: 64 KB, 4-way, hit time = 4 ciclos, miss rate = 6%\nAmbas com miss penalty = 50 ciclos. Apesar de a Config B ter miss rate menor, seu AMAT é maior. Como isso é possível?');
  q6.setRequired(true);
  q6.setChoices([
    q6.createChoice('A) AMAT = Hit Time + Miss Rate × Miss Penalty. O Hit Time 4× maior da Config B pode dominar o cálculo. Uma cache maior pode reduzir misses mas aumentar a latência de cada hit — se o aumento do hit time superar a redução do miss rate, o AMAT total piora.'),
    q6.createChoice('B) É um erro de configuração — hit time nunca pode ser maior que 1 ciclo em caches L1 reais.'),
    q6.createChoice('C) Indica que a miss penalty domina completamente o cálculo — o hit time não influencia o AMAT de forma significativa em programas reais.'),
    q6.createChoice('D) O RAVEN calcula o AMAT incorretamente quando o hit time é maior que 2 ciclos; é uma limitação conhecida do simulador.'),
    q6.createChoice('E) Não sei')
  ]);

  var q7 = form.addMultipleChoiceItem();
  q7.setTitle('Questão 7 — Working set vs. associatividade de cache\n\nUma D-Cache com 4 sets e 2 ways executa um loop que acessa ciclicamente 5 endereços (A, B, C, D, E) todos mapeando para o mesmo set. Tanto LRU (Least Recently Used) quanto FIFO (First In, First Out) produzem 100% de miss rate. O que explica esse resultado?');
  q7.setRequired(true);
  q7.setChoices([
    q7.createChoice('A) O problema é a política de escrita — ao usar write-through em vez de write-back, as linhas seriam preservadas entre iterações e o miss rate cairia.'),
    q7.createChoice('B) A cache de 2 ways é muito pequena para qualquer programa com 5 endereços distintos — seria necessário aumentar o tamanho total para 5× o tamanho do bloco atual.'),
    q7.createChoice('C) Com apenas 2 ways e 5 endereços competindo pelo mesmo set, nenhuma política de substituição pode evitar misses — o working set (5 blocos) excede a associatividade disponível (2 ways). É um miss de conflito estrutural que só seria resolvido aumentando a associatividade para 5 ways ou mais.'),
    q7.createChoice('D) O resultado indica um erro de mapeamento de endereços — com 4 sets disponíveis, os 5 endereços deveriam se distribuir entre sets diferentes, eliminando os conflitos.'),
    q7.createChoice('E) Não sei')
  ]);

  var q8 = form.addMultipleChoiceItem();
  q8.setTitle('Questão 8 — Função do estágio WB no pipeline de cinco estágios\n\nNo pipeline: IF → ID → EX → MEM → WB. Qual é a função primária do estágio WB (Write-Back)?');
  q8.setHelpText('Siglas dos estágios: IF = Instruction Fetch; ID = Instruction Decode; EX = Execute; MEM = Memory Access; WB = Write-Back. PC significa Program Counter (contador de programa).');
  q8.setRequired(true);
  q8.setChoices([
    q8.createChoice('A) Ler os operandos do banco de registradores (rs1, rs2) e detectar dependências de dados entre instruções em voo.'),
    q8.createChoice('B) Escrever o resultado da operação no registrador de destino (rd) especificado pela instrução.'),
    q8.createChoice('C) Buscar a próxima instrução na memória de instruções usando o valor atual do PC.'),
    q8.createChoice('D) Executar a operação aritmética ou lógica na ULA sobre os operandos preparados no estágio anterior.'),
    q8.createChoice('E) Não sei')
  ]);

  var q9 = form.addMultipleChoiceItem();
  q9.setTitle('Questão 9 — Pipeline aumenta throughput, não reduz latência individual\n\nUm aluno afirma: "O pipeline do RAVEN acelera o processamento porque cada instrução individual é executada mais rápido." Por que essa afirmação está incorreta?');
  q9.setRequired(true);
  q9.setChoices([
    q9.createChoice('A) Está incorreta porque o pipeline reduz a frequência de clock para acomodar os múltiplos estágios, e o ganho de desempenho vem apenas do paralelismo entre programas diferentes rodando simultaneamente.'),
    q9.createChoice('B) Está incorreta porque o ganho de desempenho vem exclusivamente do cache de instruções (I-Cache) — o pipeline em si não contribui.'),
    q9.createChoice('C) Está incorreta porque o pipeline aumenta o throughput (instruções finalizadas por unidade de tempo) ao sobrepor execução de múltiplas instruções em estágios distintos, mas a latência individual de cada instrução permanece a mesma ou até aumenta levemente.'),
    q9.createChoice('D) Está incorreta porque o pipeline só acelera instruções aritméticas; instruções de memória e branches têm a mesma latência em execução pipelined e sequencial.'),
    q9.createChoice('E) Não sei')
  ]);

  var q10 = form.addMultipleChoiceItem();
  q10.setTitle('Questão 10 — Load-Use Hazard com forwarding ativo\n\nCom pipeline e forwarding ativados, o seguinte trecho é executado:\n  lw  x5, 0(x1)   ← carrega da memória para x5\n  add x6, x5, x2  ← usa x5 imediatamente\nUma bolha é inserida entre as duas instruções. Qual é a causa correta?');
  q10.setHelpText('Nesta questão, WAW significa Write-After-Write.');
  q10.setRequired(true);
  q10.setChoices([
    q10.createChoice('A) O forwarding não foi ativado corretamente — a bolha indica que o dado de x5 não foi encaminhado do estágio EX para o ID.'),
    q10.createChoice('B) O lw e o add formam um hazard WAW (Write-After-Write), pois ambas as instruções envolvem o registrador x5, forçando o pipeline a serializar as escritas.'),
    q10.createChoice('C) O branch predictor detectou um possível desvio de controle e inseriu a bolha preventivamente antes de confirmar que não há branch.'),
    q10.createChoice('D) O lw produz o valor de x5 somente ao final do estágio MEM, mas o add precisa desse valor no início de EX — um ciclo antes. Nenhum caminho de bypass pode fazer o dado chegar antes de estar pronto: exatamente 1 stall é inevitável no load-use hazard, mesmo com todos os caminhos de forwarding ativos.'),
    q10.createChoice('E) Não sei')
  ]);

  var q11 = form.addMultipleChoiceItem();
  q11.setTitle('Questão 11 — CPI: ciclos por instrução, não contagem de instruções\n\nComparando dois programas:\n• Programa α: add e xor entre registradores independentes → CPI = 1,05\n• Programa β: cada instrução usa o resultado da anterior → CPI = 2,4\nUm aluno conclui: "Programa β tem mais instruções, por isso tem CPI mais alto." O que há de errado?');
  q11.setHelpText('Nesta questão, CPI significa Cycles Per Instruction (ciclos por instrução) e RAW significa Read-After-Write.');
  q11.setRequired(true);
  q11.setChoices([
    q11.createChoice('A) O CPI mede ciclos por instrução — não a quantidade de instruções. O CPI 2,4 indica que cada instrução consome em média 2,4 ciclos devido a stalls por dependências RAW. Os dois programas podem ter o mesmo número de instruções; o custo extra vem das bolhas inseridas no pipeline.'),
    q11.createChoice('B) O raciocínio está correto — mais instruções sempre resultam em CPI mais alto, pois o pipeline fica mais ocupado.'),
    q11.createChoice('C) O CPI é uma métrica de consumo de energia, não de tempo.'),
    q11.createChoice('D) O aluno confundiu CPI com IPC — um CPI de 2,4 na verdade significa 2,4 instruções por ciclo.'),
    q11.createChoice('E) Não sei')
  ]);

  var q12 = form.addMultipleChoiceItem();
  q12.setTitle('Questão 12 — Por que forwarding não elimina o stall do load-use hazard\n\nUm aluno ativa o forwarding esperando eliminar todos os stalls. Por que isso não é possível para um lw seguido imediatamente da instrução que usa o valor carregado?');
  q12.setRequired(true);
  q12.setChoices([
    q12.createChoice('A) O forwarding só pode encaminhar um resultado quando ele já está disponível. Para um lw, o dado da memória só existe ao final do estágio MEM — mas a instrução seguinte precisa desse valor no início de EX, um ciclo antes. Exatamente 1 stall é inevitável no load-use hazard, mesmo com todos os caminhos de forwarding ativos.'),
    q12.createChoice('B) O forwarding resolve o load-use hazard normalmente; se ainda há stalls, é porque o compilador não reorganizou as instruções.'),
    q12.createChoice('C) O forwarding é desativado automaticamente para instruções de load no RAVEN porque causaria conflito no barramento interno do pipeline.'),
    q12.createChoice('D) O stall extra no load-use existe para proteger a integridade da D-Cache.'),
    q12.createChoice('E) Não sei')
  ]);

  var q13 = form.addMultipleChoiceItem();
  q13.setTitle('Questão 13 — Stall vs. Flush: causas diferentes, efeito visual parecido\n\nNo painel de pipeline do RAVEN, o aluno observa: (1) uma instrução permanece parada em estágios iniciais enquanto estágios à frente recebem bolhas; (2) várias instruções já avançadas no pipeline são descartadas e substituídas por bolhas. Por que esses dois eventos têm causas fundamentalmente diferentes?');
  q13.setRequired(true);
  q13.setChoices([
    q13.createChoice('A) Os dois eventos são a mesma coisa — termos distintos para qualquer ciclo improdutivo, sem diferença de causa ou mecanismo.'),
    q13.createChoice('B) O primeiro ocorre apenas sem forwarding; o segundo só ocorre com forwarding ativado — são mutuamente exclusivos por design.'),
    q13.createChoice('C) O primeiro é um stall (hazard RAW): o pipeline insere bolhas para aguardar o resultado ficar disponível. O segundo é um flush/squash (hazard de controle): após detectar uma misprediction de branch ou exceção, o pipeline descarta instruções buscadas por um caminho errado. Causas diferentes, mecanismos distintos.'),
    q13.createChoice('D) O primeiro ocorre quando a D-Cache tem um miss; o segundo, quando a I-Cache tem um miss — ambos têm origem no subsistema de memória.'),
    q13.createChoice('E) Não sei')
  ]);

  var q14 = form.addMultipleChoiceItem();
  q14.setTitle('Questão 14 — Identificar o tipo de hazard de dados em um trecho de código\n\nCom pipeline habilitado, o seguinte trecho gera uma dependência destacada:\n  mul x5, x1, x2  ← rd=x5\n  add x6, x5, x3  ← rs1=x5\nQual tipo de hazard de dados está sendo detectado, e por quê?');
  q14.setHelpText('Siglas desta questão: WAW = Write-After-Write; WAR = Write-After-Read; RAW = Read-After-Write.');
  q14.setRequired(true);
  q14.setChoices([
    q14.createChoice('A) WAW (Write-After-Write): tanto mul quanto add escrevem em registradores, criando um conflito de escrita.'),
    q14.createChoice('B) WAR (Write-After-Read): o add lê x5 e só depois o mul termina de escrevê-lo.'),
    q14.createChoice('C) Nenhum hazard relevante: as instruções escrevem em registradores de destino diferentes (x5 e x6).'),
    q14.createChoice('D) RAW (Read-After-Write): o add precisa ler x5 (rs1) antes que o mul tenha terminado de escrevê-lo (rd). Em um pipeline em-ordem, o mul ainda está em EX ou MEM quando o add chega ao estágio que precisa do valor.'),
    q14.createChoice('E) Não sei')
  ]);

  var q15 = form.addMultipleChoiceItem();
  q15.setTitle('Questão 15 — Por que WAR e WAW não causam stalls em pipelines em-ordem\n\nApós executar um programa no RAVEN, as estatísticas mostram: hazards RAW aparecem dezenas de vezes; WAR e WAW aparecem zero vezes. O aluno suspeita de um bug. Por que essa distribuição é correta e esperada?');
  q15.setHelpText('Siglas desta questão: RAW = Read-After-Write; WAR = Write-After-Read; WAW = Write-After-Write.');
  q15.setRequired(true);
  q15.setChoices([
    q15.createChoice('A) É um bug — em qualquer pipeline real, os três tipos ocorrem com frequência semelhante.'),
    q15.createChoice('B) Em pipelines em-ordem, as instruções são sempre lidas no estágio ID antes de escritas no WB, e cada instrução termina na mesma sequência em que foi buscada. WAR e WAW só seriam problemáticos se uma instrução posterior pudesse completar antes de uma anterior — o que não ocorre em pipelines em-ordem. Apenas RAW cria stalls reais.'),
    q15.createChoice('C) WAR e WAW aparecem como zero porque o RAVEN simplifica o modelo de hazards para fins didáticos.'),
    q15.createChoice('D) WAR e WAW só ocorrem em programas com laços muito longos — para programas curtos, é normal que não apareçam.'),
    q15.createChoice('E) Não sei')
  ]);

  var q16 = form.addMultipleChoiceItem();
  q16.setTitle('Questão 16 — O que o CPI mede e o que ele não mede\n\nNa aba Run do RAVEN, o painel exibe "CPI: 1,85". Um aluno interpreta: "Cada instrução demorou 1,85 segundos para executar." O que essa leitura significa corretamente, e por que a interpretação está errada?');
  q16.setHelpText('Nesta questão, CPI significa Cycles Per Instruction (ciclos por instrução).');
  q16.setRequired(true);
  q16.setChoices([
    q16.createChoice('A) A interpretação está errada porque CPI é medido em nanosegundos, não segundos.'),
    q16.createChoice('B) CPI (Cycles Per Instruction) é a média de ciclos de clock gastos por instrução. Um CPI de 1,85 significa que cada instrução consumiu em média 1,85 ciclos. O tempo real de execução depende também da frequência do clock (t = N × CPI / frequência) e não pode ser lido diretamente do CPI.'),
    q16.createChoice('C) A interpretação está parcialmente correta: ciclos modernos duram exatamente 1 nanosegundo, então o valor em ns seria numericamente o mesmo.'),
    q16.createChoice('D) CPI de 1,85 significa que o pipeline executou 1,85 instruções por ciclo (IPC = 1,85).'),
    q16.createChoice('E) Não sei')
  ]);

  var q17 = form.addMultipleChoiceItem();
  q17.setTitle('Questão 17 — Por que o speedup do pipeline não é linear com o número de estágios\n\nUm aluno calcula que um pipeline de 5 estágios deveria oferecer speedup de 5×. Ao comparar no RAVEN pipeline habilitado vs. desabilitado, obtém speedup real de apenas 2,8×. Qual combinação de fatores explica a diferença?');
  q17.setRequired(true);
  q17.setChoices([
    q17.createChoice('A) O speedup é limitado principalmente pelo barramento de memória — em execução sequencial, a memória opera a 5× a velocidade do pipeline.'),
    q17.createChoice('B) O speedup real seria 5× se o programa não contivesse instruções de branch — branches são o único fator limitante.'),
    q17.createChoice('C) O speedup de 5× assume que o RAVEN paraleliza apenas instruções independentes; instruções com qualquer dependência são sempre serializadas.'),
    q17.createChoice('D) O speedup teórico de N× assume pipeline perfeito sem overhead. Na prática, é reduzido por: (1) stalls por hazards RAW e load-use; (2) flushes por misprediction de branch; (3) miss penalties de cache; e (4) desequilíbrio entre estágios. O CPI observado reflete a soma de todos esses overheads.'),
    q17.createChoice('E) Não sei')
  ]);

  // ═══════════════════════════════════════════════════════════════════════════
  // SEÇÃO 3 — Atividade Guiada no RAVEN
  // ═══════════════════════════════════════════════════════════════════════════

  var pg3 = form.addPageBreakItem();
  pg3.setTitle('Seção 3 — Atividade Guiada no RAVEN (≈65 min)');
  pg3.setHelpText(
    'Esta seção acompanha a atividade prática com o simulador RAVEN.\n' +
    'Para cada domínio, siga as orientações do aplicador e registre o que foi observado.\n' +
    'Quando houver campos numéricos, informe apenas os valores observados.\n' +
    'Quando houver observação visual, foque apenas no trecho ou momento indicado.\n' +
    'Use ponto ou vírgula para decimais.\n' +
    'As respostas descritivas continuam focadas em observação, não em acertar.\n' +
    'Os arquivos desta atividade estão organizados em:\n' +
    '• "guided-activity/programas"\n' +
    '• "guided-activity/config-global"\n' +
    '• "guided-activity/config-pipeline"\n' +
    '• "guided-activity/config-cache"\n' +
    'Sempre importe primeiro o arquivo da aba Config (.rcfg).\n' +
    'Depois, importe o da aba Pipeline (.pcfg).\n' +
    'Por fim, importe o da aba Cache (.fcache), quando houver.'
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
    'Preset: D1-02 (mantém R100 + P100, troca para D102).\n\n' +
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
    'Pressione Ctrl+Enter para recompilar. Execute novamente e observe o par lw/nop/add no diagrama.\n\n' +
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

  form.addSectionHeaderItem().setTitle('D3 — Cache (≈15 min)');

  // D3 — Experimento 1: Tamanho vs. Latência de Acesso (AMAT)

  var d3p1a = form.addTextItem();
  d3p1a.setTitle('AMAT (Average Memory Access Time) — Config A: cache pequena, hit time baixo');
  d3p1a.setHelpText(
    'Preset: D3-01 (carrega R300 + P101 + C311 + D301 automaticamente).\n\n' +
    'Execute o programa e leia o AMAT da D-Cache no resumo da aba Cache.\n' +
    'AMAT = Hit Time + Miss Rate × Miss Penalty.\n\n' +
    'Informe o valor observado (em ciclos).'
  );
  d3p1a.setValidation(criarValidacaoNumeroDecimal('Informe apenas o AMAT da Config A em ciclos.'));
  d3p1a.setRequired(true);

  var d3p1b = form.addTextItem();
  d3p1b.setTitle('AMAT — Config B: cache maior, hit time mais alto');
  d3p1b.setHelpText(
    'Preset: D3-02 (mantém R300 + P101 + D301, troca para C312).\n\n' +
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
    d3p1c.createChoice('Config A (16 KB, hit time 1 ciclo)'),
    d3p1c.createChoice('Config B (64 KB, hit time 4 ciclos)'),
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

  // D3 — Experimento 3: Associatividade e Working Set (Thrashing)

  var d3p3a = form.addTextItem();
  d3p3a.setTitle('Miss rate com cache de 2 ways (associatividade baixa)');
  d3p3a.setHelpText(
    'Preset: D3-05 (carrega R300 + P101 + C331 + D302).\n\n' +
    'D302 acessa ciclicamente 5 endereços diferentes. A cache tem 4 sets com 2 ways cada.\n' +
    'Execute e leia o D-Cache miss rate no resumo da aba Cache.\n\n' +
    'Informe o miss rate observado.'
  );
  d3p3a.setValidation(criarValidacaoNumeroDecimal('Informe apenas o miss rate observado com 2 ways.'));
  d3p3a.setRequired(true);

  var d3p3b = form.addMultipleChoiceItem();
  d3p3b.setTitle('Ao aumentar a associatividade de 2 ways para 8 ways, o que acontece com o miss rate?');
  d3p3b.setHelpText(
    'Preset: D3-06 (mantém R300 + P101 + D302, troca para C332).\n\n' +
    'Execute e compare o miss rate com o resultado anterior.'
  );
  d3p3b.setRequired(true);
  d3p3b.setChoices([
    d3p3b.createChoice('Diminui'),
    d3p3b.createChoice('Permanece igual'),
    d3p3b.createChoice('Aumenta')
  ]);

  var d3p3c = form.addParagraphTextItem();
  d3p3c.setTitle('Thrashing por working set — por que aumentar a associatividade resolveu (ou não) o problema?');
  d3p3c.setHelpText(
    'Compare os resultados de C331 (2 ways) e C332 (8 ways) com D302.\n\n' +
    'O que mudou entre as duas configurações? O que o miss rate indica sobre a relação\n' +
    'entre o padrão de acesso do programa e a configuração de cache?'
  );
  d3p3c.setRequired(true);

  // ── D4: ISA RISC-V ────────────────────────────────────────────────────────

  form.addSectionHeaderItem().setTitle('D4 — ISA (Instruction Set Architecture) RISC-V (≈8 min)');

  var d4p1 = form.addParagraphTextItem();
  d4p1.setTitle('Codificação binária: por que S-type não tem campo rd?');
  d4p1.setHelpText(
    'Preset: D4-01 (carrega R100 + P101 + D401 automaticamente).\n\n' +
    'No Editor, clique sobre a instrução add e depois sobre a instrução sw.\n' +
    'O painel de detalhes mostra a codificação binária de cada uma.\n\n' +
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
    'responda de forma independente. Tempo estimado: 20 minutos.'
  );

  var pos1 = form.addMultipleChoiceItem();
  pos1.setTitle('Questão 1 — Formatos de instrução RISC-V\n\nQuais formatos de instrução RISC-V não possuem o campo rd (registrador de destino) em sua codificação binária?');
  pos1.setRequired(true);
  pos1.setChoices([
    pos1.createChoice('A) R-type (add, sub, mul) e I-type (addi, lw, jalr) — porque operações com dois registradores de fonte nunca precisam de destino explícito na codificação binária.'),
    pos1.createChoice('B) S-type (sw, sb) e B-type (beq, bne) — porque produzem efeito em memória (store) ou no PC (branch) e não escrevem em nenhum registrador de destino. Os bits [11:7] codificam parte do imediato nesses formatos.'),
    pos1.createChoice('C) U-type (upper immediate) e J-type (jump) — porque trabalham com imediatos de 20 bits que ocupam todo o espaço disponível, inclusive os bits onde normalmente estaria o rd.'),
    pos1.createChoice('D) Apenas ECALL e EBREAK — por serem instruções de sistema tratadas de forma especial pelo hardware, sem registrador de destino definido.')
  ]);

  var pos2 = form.addMultipleChoiceItem();
  pos2.setTitle('Questão 2 — Desempenho RISC vs. CISC\n\nUm aluno conclui: "Processadores RISC precisam de mais instruções para realizar o mesmo trabalho e, portanto, são mais lentos que CISC." Por que esse argumento é uma simplificação incorreta?');
  pos2.setHelpText('Nesta questão, CPI significa Cycles Per Instruction (ciclos por instrução).');
  pos2.setRequired(true);
  pos2.setChoices([
    pos2.createChoice('A) O argumento está correto — arquiteturas RISC executam mais instruções que CISC, mas compensam com frequências de clock muito mais altas que dobram o desempenho de forma consistente.'),
    pos2.createChoice('B) Está incorreto porque o CPI de processadores RISC é sempre exatamente 1 — como cada instrução completa em 1 ciclo, o número maior de instruções se compensa automaticamente.'),
    pos2.createChoice('C) Está incorreto porque o número de instruções por programa não é um bom indicador de desempenho isoladamente — o que importa é o tempo total (N × CPI × período de clock). A regularidade RISC viabiliza pipelines mais profundos e frequências mais altas.'),
    pos2.createChoice('D) Está incorreto apenas para operações de ponto flutuante — para operações inteiras puras, arquiteturas CISC de fato superam RISC em desempenho absoluto por instrução.')
  ]);

  var pos3 = form.addMultipleChoiceItem();
  pos3.setTitle('Questão 3 — Contexto de execução em multi-core\n\nDois cores executam programas em paralelo. Ao inspecionar o banco de registradores do Core 0 e do Core 1, os valores são completamente diferentes. Por que isso é correto e esperado?');
  pos3.setHelpText('Nesta questão, PC significa Program Counter (contador de programa).');
  pos3.setRequired(true);
  pos3.setChoices([
    pos3.createChoice('A) Os dois cores compartilham o mesmo banco de registradores físico — o que difere é apenas o PC.'),
    pos3.createChoice('B) As diferenças são temporárias — os bancos sincronizam automaticamente via cache coherence ao fim de cada fase de execução.'),
    pos3.createChoice('C) Os registradores diferem porque os dois cores operam em frequências de clock distintas.'),
    pos3.createChoice('D) Cada core possui seu próprio banco de registradores privado (x0–x31), seu próprio PC e estado interno completamente independentes. O que compartilham é apenas a memória principal (RAM) e o subsistema de cache.')
  ]);

  var pos4 = form.addMultipleChoiceItem();
  pos4.setTitle('Questão 4 — Função do campo tag no endereçamento de cache\n\nUm endereço de memória é dividido em: offset, índice e tag. Qual desses campos verifica se o bloco armazenado em um set corresponde ao endereço sendo acessado?');
  pos4.setRequired(true);
  pos4.setChoices([
    pos4.createChoice('A) O campo índice, porque identifica unicamente o set e — em mapeamento direto — também identifica unicamente o bloco armazenado naquele set.'),
    pos4.createChoice('B) O campo offset, porque indica a posição exata dentro do bloco.'),
    pos4.createChoice('C) O campo tag, porque múltiplos endereços distintos mapeiam para o mesmo set. A tag carrega os bits de alta ordem e permite confirmar se o bloco presente naquele set pertence ao endereço sendo acessado.'),
    pos4.createChoice('D) Nenhum campo adicional é necessário em mapeamento direto — o índice já garante que o bloco é o correto.')
  ]);

  var pos5 = form.addMultipleChoiceItem();
  pos5.setTitle('Questão 5 — Limitação da política LRU (Least Recently Used) em varredura sequencial\n\nPor que a política LRU falha para programas que fazem varreduras sequenciais de arrays grandes (padrão streaming)?');
  pos5.setHelpText('Nesta questão, FIFO significa First In, First Out.');
  pos5.setRequired(true);
  pos5.setChoices([
    pos5.createChoice('A) Falha porque o LRU exige mais hardware que FIFO (First In, First Out) — o overhead aumenta o hit time a ponto de superar o ganho em hit rate.'),
    pos5.createChoice('B) Falha porque em streaming o programa acessa cada elemento exatamente uma vez, em ordem, sem retornar ao mesmo bloco no curto prazo. A localidade temporal simplesmente não existe em streaming.'),
    pos5.createChoice('C) Falha apenas quando o array não cabe inteiramente na cache.'),
    pos5.createChoice('D) Falha porque varreduras sequenciais ativam um modo de prefetch automático que contorna a política LRU.')
  ]);

  var pos6 = form.addMultipleChoiceItem();
  pos6.setTitle('Questão 6 — AMAT (Average Memory Access Time / tempo médio de acesso à memória) e o trade-off entre tamanho e latência de cache\n\n• Config A: 16 KB, 4-way, hit time = 1 ciclo, miss rate = 8%\n• Config B: 64 KB, 4-way, hit time = 4 ciclos, miss rate = 6%\nAmbas com miss penalty = 50 ciclos. Como o AMAT da Config B pode ser maior mesmo com miss rate menor?');
  pos6.setRequired(true);
  pos6.setChoices([
    pos6.createChoice('A) AMAT = Hit Time + Miss Rate × Miss Penalty. O Hit Time 4× maior da Config B pode dominar o cálculo — se o aumento do hit time superar a redução do miss rate, o AMAT total piora.'),
    pos6.createChoice('B) É um erro de configuração — hit time nunca pode ser maior que 1 ciclo em caches L1 reais.'),
    pos6.createChoice('C) Indica que a miss penalty domina completamente — o hit time não influencia o AMAT de forma significativa.'),
    pos6.createChoice('D) O RAVEN calcula o AMAT incorretamente quando o hit time é maior que 2 ciclos.')
  ]);

  var pos7 = form.addMultipleChoiceItem();
  pos7.setTitle('Questão 7 — Working set vs. associatividade de cache\n\nUma D-Cache com 4 sets e 2 ways executa um loop que acessa ciclicamente 5 endereços mapeados para o mesmo set. Tanto LRU (Least Recently Used) quanto FIFO (First In, First Out) produzem 100% de miss rate. O que explica esse resultado?');
  pos7.setRequired(true);
  pos7.setChoices([
    pos7.createChoice('A) O problema é a política de escrita — ao usar write-back em vez de write-through, as linhas seriam preservadas e o miss rate cairia.'),
    pos7.createChoice('B) A cache de 2 ways é muito pequena — seria necessário aumentar o tamanho total para 5× o tamanho do bloco atual.'),
    pos7.createChoice('C) Com apenas 2 ways e 5 endereços competindo pelo mesmo set, nenhuma política de substituição pode evitar misses — o working set (5 blocos) excede a associatividade disponível (2 ways). É um miss de conflito estrutural.'),
    pos7.createChoice('D) O resultado indica um erro de mapeamento de endereços — com 4 sets disponíveis, os 5 endereços deveriam se distribuir entre sets diferentes.')
  ]);

  var pos8 = form.addMultipleChoiceItem();
  pos8.setTitle('Questão 8 — Função do estágio WB no pipeline de cinco estágios\n\nNo pipeline IF → ID → EX → MEM → WB. Qual é a função primária do estágio WB?');
  pos8.setHelpText('Siglas dos estágios: IF = Instruction Fetch; ID = Instruction Decode; EX = Execute; MEM = Memory Access; WB = Write-Back. PC significa Program Counter (contador de programa).');
  pos8.setRequired(true);
  pos8.setChoices([
    pos8.createChoice('A) Ler os operandos do banco de registradores (rs1, rs2) e detectar dependências de dados entre instruções em voo.'),
    pos8.createChoice('B) Escrever o resultado da operação no registrador de destino (rd) especificado pela instrução.'),
    pos8.createChoice('C) Buscar a próxima instrução na memória de instruções usando o valor atual do PC.'),
    pos8.createChoice('D) Executar a operação aritmética ou lógica na ULA sobre os operandos preparados no estágio anterior.')
  ]);

  var pos9 = form.addMultipleChoiceItem();
  pos9.setTitle('Questão 9 — Pipeline aumenta throughput, não reduz latência individual\n\nPor que a afirmação "o pipeline acelera o processamento porque cada instrução individual é executada mais rápido" está incorreta?');
  pos9.setRequired(true);
  pos9.setChoices([
    pos9.createChoice('A) Está incorreta porque o pipeline reduz a frequência de clock para acomodar os múltiplos estágios, e o ganho vem apenas do paralelismo entre programas diferentes.'),
    pos9.createChoice('B) Está incorreta porque o ganho vem exclusivamente do cache de instruções (I-Cache) — o pipeline em si não contribui.'),
    pos9.createChoice('C) Está incorreta porque o pipeline aumenta o throughput (instruções finalizadas por unidade de tempo) ao sobrepor execução de múltiplas instruções em estágios distintos, mas a latência individual de cada instrução permanece a mesma ou até aumenta levemente.'),
    pos9.createChoice('D) Está incorreta porque o pipeline só acelera instruções aritméticas; instruções de memória e branches têm a mesma latência em execução pipelined e sequencial.')
  ]);

  var pos10 = form.addMultipleChoiceItem();
  pos10.setTitle('Questão 10 — Load-Use Hazard com forwarding ativo\n\n  lw  x5, 0(x1)\n  add x6, x5, x2\n\nCom pipeline e forwarding ativos, por que uma bolha ainda é inserida?');
  pos10.setHelpText('Nesta questão, WAW significa Write-After-Write.');
  pos10.setRequired(true);
  pos10.setChoices([
    pos10.createChoice('A) O forwarding não foi ativado corretamente — a bolha indica que o dado de x5 não foi encaminhado do estágio EX para o ID.'),
    pos10.createChoice('B) Formam um hazard WAW (Write-After-Write), forçando o pipeline a serializar as escritas.'),
    pos10.createChoice('C) O branch predictor detectou um possível desvio e inseriu a bolha preventivamente.'),
    pos10.createChoice('D) O lw produz o valor de x5 somente ao final do estágio MEM, mas o add precisa desse valor no início de EX — um ciclo antes. Exatamente 1 stall é inevitável no load-use hazard, mesmo com todos os caminhos de forwarding ativos.')
  ]);

  var pos11 = form.addMultipleChoiceItem();
  pos11.setTitle('Questão 11 — CPI: ciclos por instrução, não contagem de instruções\n\n• Programa α: add e xor independentes → CPI = 1,05\n• Programa β: cada instrução usa o resultado da anterior → CPI = 2,4\n\nUm aluno conclui: "Programa β tem mais instruções, por isso tem CPI mais alto." O que há de errado?');
  pos11.setHelpText('Nesta questão, CPI significa Cycles Per Instruction (ciclos por instrução) e RAW significa Read-After-Write.');
  pos11.setRequired(true);
  pos11.setChoices([
    pos11.createChoice('A) O CPI mede ciclos por instrução — não a quantidade de instruções. O CPI 2,4 indica que cada instrução consome em média 2,4 ciclos devido a stalls por dependências RAW. Os dois programas podem ter o mesmo número de instruções; o custo extra vem das bolhas inseridas no pipeline.'),
    pos11.createChoice('B) O raciocínio está correto — mais instruções sempre resultam em CPI mais alto.'),
    pos11.createChoice('C) O CPI é uma métrica de consumo de energia, não de tempo.'),
    pos11.createChoice('D) O aluno confundiu CPI com IPC — um CPI de 2,4 na verdade significa 2,4 instruções por ciclo.')
  ]);

  var pos12 = form.addMultipleChoiceItem();
  pos12.setTitle('Questão 12 — Por que forwarding não elimina o stall do load-use hazard\n\nPor que ativar o forwarding não elimina o stall de um lw seguido imediatamente da instrução que usa o valor carregado?');
  pos12.setRequired(true);
  pos12.setChoices([
    pos12.createChoice('A) O forwarding só pode encaminhar um resultado quando ele já está disponível. Para um lw, o dado da memória só existe ao final do estágio MEM — mas a instrução seguinte precisa desse valor no início de EX, um ciclo antes. Exatamente 1 stall é inevitável, mesmo com todos os caminhos de forwarding ativos.'),
    pos12.createChoice('B) O forwarding resolve o load-use hazard normalmente; se ainda há stalls, é porque o compilador não reorganizou as instruções.'),
    pos12.createChoice('C) O forwarding é desativado automaticamente para instruções de load no RAVEN porque causaria conflito no barramento interno do pipeline.'),
    pos12.createChoice('D) O stall extra no load-use existe para proteger a integridade da D-Cache.')
  ]);

  var pos13 = form.addMultipleChoiceItem();
  pos13.setTitle('Questão 13 — Stall vs. Flush: causas diferentes, efeito visual parecido\n\nPor que (1) instrução parada com bolhas à frente e (2) várias instruções avançadas sendo descartadas têm causas fundamentalmente diferentes?');
  pos13.setRequired(true);
  pos13.setChoices([
    pos13.createChoice('A) Os dois eventos são a mesma coisa — termos distintos para qualquer ciclo improdutivo, sem diferença de causa ou mecanismo.'),
    pos13.createChoice('B) O primeiro ocorre apenas sem forwarding; o segundo só ocorre com forwarding ativado — são mutuamente exclusivos por design.'),
    pos13.createChoice('C) O primeiro é um stall (hazard RAW): o pipeline insere bolhas para aguardar o resultado ficar disponível. O segundo é um flush/squash (hazard de controle): o pipeline descarta instruções buscadas por um caminho errado após detectar misprediction de branch ou exceção.'),
    pos13.createChoice('D) O primeiro ocorre quando a D-Cache tem um miss; o segundo, quando a I-Cache tem um miss — ambos têm origem no subsistema de memória.')
  ]);

  var pos14 = form.addMultipleChoiceItem();
  pos14.setTitle('Questão 14 — Identificar o tipo de hazard de dados em um trecho de código\n\n  mul x5, x1, x2  ← rd=x5\n  add x6, x5, x3  ← rs1=x5\n\nQual tipo de hazard de dados está sendo detectado, e por quê?');
  pos14.setHelpText('Siglas desta questão: WAW = Write-After-Write; WAR = Write-After-Read; RAW = Read-After-Write.');
  pos14.setRequired(true);
  pos14.setChoices([
    pos14.createChoice('A) WAW (Write-After-Write): tanto mul quanto add escrevem em registradores, criando um conflito de escrita.'),
    pos14.createChoice('B) WAR (Write-After-Read): o add lê x5 e só depois o mul termina de escrevê-lo.'),
    pos14.createChoice('C) Nenhum hazard relevante: as instruções escrevem em registradores de destino diferentes (x5 e x6).'),
    pos14.createChoice('D) RAW (Read-After-Write): o add precisa ler x5 (rs1) antes que o mul tenha terminado de escrevê-lo (rd). Em um pipeline em-ordem, o mul ainda está em EX ou MEM quando o add chega ao estágio que precisa do valor.')
  ]);

  var pos15 = form.addMultipleChoiceItem();
  pos15.setTitle('Questão 15 — Por que WAR e WAW não causam stalls em pipelines em-ordem\n\nAs estatísticas mostram: hazards RAW aparecem dezenas de vezes; WAR e WAW aparecem zero vezes. Por que essa distribuição é correta e esperada?');
  pos15.setHelpText('Siglas desta questão: RAW = Read-After-Write; WAR = Write-After-Read; WAW = Write-After-Write.');
  pos15.setRequired(true);
  pos15.setChoices([
    pos15.createChoice('A) É um bug — em qualquer pipeline real, os três tipos ocorrem com frequência semelhante.'),
    pos15.createChoice('B) Em pipelines em-ordem, as instruções são sempre lidas no estágio ID antes de escritas no WB, e cada instrução termina na mesma sequência em que foi buscada. WAR e WAW só seriam problemáticos se uma instrução posterior pudesse completar antes de uma anterior — o que não ocorre em pipelines em-ordem.'),
    pos15.createChoice('C) WAR e WAW aparecem como zero porque o RAVEN simplifica o modelo de hazards para fins didáticos.'),
    pos15.createChoice('D) WAR e WAW só ocorrem em programas com laços muito longos.')
  ]);

  var pos16 = form.addMultipleChoiceItem();
  pos16.setTitle('Questão 16 — O que o CPI mede e o que ele não mede\n\nO painel exibe "CPI: 1,85". Por que a interpretação "cada instrução demorou 1,85 segundos" está errada?');
  pos16.setHelpText('Nesta questão, CPI significa Cycles Per Instruction (ciclos por instrução).');
  pos16.setRequired(true);
  pos16.setChoices([
    pos16.createChoice('A) CPI é medido em nanosegundos, não segundos.'),
    pos16.createChoice('B) CPI (Cycles Per Instruction) é a média de ciclos de clock gastos por instrução. Um CPI de 1,85 significa que cada instrução consumiu em média 1,85 ciclos. O tempo real de execução depende também da frequência do clock (t = N × CPI / frequência) e não pode ser lido diretamente do CPI.'),
    pos16.createChoice('C) A interpretação está parcialmente correta: ciclos modernos duram exatamente 1 nanosegundo, então o valor em ns seria numericamente o mesmo.'),
    pos16.createChoice('D) CPI de 1,85 significa que o pipeline executou 1,85 instruções por ciclo (IPC = 1,85).')
  ]);

  var pos17 = form.addMultipleChoiceItem();
  pos17.setTitle('Questão 17 — Por que o speedup do pipeline não é linear\n\nUm pipeline de 5 estágios produz speedup real de apenas 2,8× em vez de 5×. Qual combinação de fatores explica a diferença?');
  pos17.setRequired(true);
  pos17.setChoices([
    pos17.createChoice('A) O speedup é limitado principalmente pelo barramento de memória.'),
    pos17.createChoice('B) O speedup real seria 5× se o programa não contivesse instruções de branch — branches são o único fator limitante.'),
    pos17.createChoice('C) O speedup de 5× assume que o RAVEN paraleliza apenas instruções independentes; instruções com qualquer dependência são sempre serializadas.'),
    pos17.createChoice('D) O speedup teórico de N× assume pipeline perfeito sem overhead. Na prática, é reduzido por: (1) stalls por hazards RAW e load-use; (2) flushes por misprediction de branch; (3) miss penalties de cache; e (4) desequilíbrio entre estágios. O CPI observado reflete a soma de todos esses overheads.')
  ]);

  // ─── Log final ─────────────────────────────────────────────────────────────
  Logger.log('✅ Formulário unificado criado com sucesso!');
  Logger.log('🔗 Link para edição: ' + form.getEditUrl());
  Logger.log('📋 Link para participantes: ' + form.getPublishedUrl());
}
