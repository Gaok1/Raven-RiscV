# FALCON-ASM Tutorial

Bem-vindo ao **tutorial da plataforma FALCON-ASM** - um guia introdutório que apresenta todas as principais funcionalidades e mecânicas da ferramenta.


## **1. Introdução**

O **FALCON-ASM** é uma plataforma que combina **IDE** e **simulador** para facilitar o aprendizado da arquitetura **RISC-V**.
Ela permite **escrever, montar e executar código assembly**, observando em tempo real o comportamento da **memória**, dos **registradores** e a **decodificação** de cada instrução.


## **2. Estrutura da Interface**

A interface do FALCON é organizada em **abas**, que representam as principais etapas do ciclo de desenvolvimento:

1. **Editor** - escrita e montagem do código.
2. **Run (Simulação)** - execução passo a passo e visualização da memória e dos registradores.
---

## **3. Aba: Editor de Texto**

<img width="1085" height="571" alt="EditorSection" src="https://github.com/user-attachments/assets/ed4d697c-61e8-4c68-b14f-605c4bd95a05" />

A aba **Editor** é o ponto de partida.
Aqui, o usuário pode **escrever, montar, importar e exportar** código assembly RISC-V.

### **Principais recursos**

* **Status da Build:** exibido na seção `Editor Status` (imagem [1]), indicando o resultado da montagem.
* **Importação e Exportação:**

  * **Arquivos de texto:** `.asm` e `.FAS`
  * **Binários:** gerados pelo próprio Falcon (.bin)
* **Atalho de Montagem:** `Ctrl + R` - monta o código e exibe o status da operação.

<img width="1102" height="571" alt="EditorSection2" src="https://github.com/user-attachments/assets/950f8d21-0b7b-4ccf-9cc0-0035fe5cfa65" />

### **Estrutura do Código**

O FALCON permite escrever instruções diretamente, sem necessidade de declarar seções.
Porém, quando há necessidade de incluir dados, utilize:

```asm
.section .data   ; ou apenas .data
```

para delimitar a região de dados, e:

```asm
.section .text   ; ou apenas .text
```

para delimitar a região de instruções.

---

## **4. Aba: Run (Simulação)**

A aba **Run** é dedicada à execução e inspeção detalhada do programa.
Aqui, o usuário pode acompanhar o comportamento da **memória**, dos **registradores** e a **decodificação binária** das instruções.

Cada instrução executada apresenta:

* **Field Map:** mapa binário de cada campo da instrução.
* **Parsed Fields:** valores binários decodificados e separados por campo.
* **Console:** permite a interação com operações de entrada e saída (I/O).

<img width="1920" height="1015" alt="image" src="https://github.com/user-attachments/assets/91c0f9c0-ad57-4519-9dd1-41633932d21b" />

---

### **Redimensionando Janelas**

As janelas da aba podem ser redimensionadas livremente.
Basta mover o cursor até a borda da janela (indicada por uma **seta**) - se ela ficar **amarela**, o redimensionamento está disponível.
Clique, segure e arraste:

* **Console:** movimenta verticalmente (cima/baixo).
* **Instruction Memory:** movimenta horizontalmente (esquerda/direita).

![WindowsTerminal\_l4T0sBIA2c](https://github.com/user-attachments/assets/6ed5d67f-46bd-4c03-80cb-cc4b393bb751)

---

### **Run Controls**

Esta seção permite controlar a execução e a forma como os dados da memória são decodificados por meio de Toggles.

* **State:** alterna entre os modos **RUN** e **PAUSE**.
* **View:** alterna a visualização entre **Registradores** e **Memória RAM**.

![WindowsTerminal\_2La84UZiCS](https://github.com/user-attachments/assets/011c2869-6cc7-4759-b3dc-59f291229796)

---

### **Janela de Memória**

A janela à esquerda da aba **Run** exibe o conteúdo da memória e dos registradores, ambos podendo ser rolados com o **scroll do mouse**.
O modo de exibição pode ser alternado com o botão `View` na seção **Run Controls**.

É possível modificar a forma de **interpretação dos dados**, visualizando-os em:

* **Hex:** formato hexadecimal.
* **Dec:** formato decimal.
* **STR:** formato ASCII (texto).

#### **Format Dec**

Para o modo decimal (**Dec**), pode-se alternar entre:

* **SGN:** números com sinal (*signed*).
* **UNS:** números sem sinal (*unsigned*).

#### **View RAM**

No modo de visualização da **RAM**, é possível navegar diretamente até regiões de interesse como:

* **.data:** onde ficam armazenados valores declarados na seção `.data`.
* **Stack:** região da pilha, apontada pelo registrador **SP (stack pointer)**.

Também é possível alternar o espaçamento de endereços exibidos entre **4B**, **2B** e **1B**.

![WindowsTerminal\_YF5tCEQbFG](https://github.com/user-attachments/assets/45bb08aa-1c36-407a-b159-df8af5d36d53)

---

### **Instruction Memory**

A janela **Instruction Memory** mostra o avanço do **PC (Program Counter)** durante a execução do programa.
A faixa **amarela** indica a instrução atualmente apontada pelo PC. As seções **Instruction Details**, **Field Map** e **Parsed Fields** refletem essa instrução em tempo real.

#### **Hover de Instrução**

Ao passar o cursor sobre uma instrução, um **bloco azul** aparece ao lado dela.
Se clicar, o PC será movido para essa instrução.
A instrução destacada pelo hover define o conteúdo exibido nas janelas **Instruction Details**, **Field Map** e **Parsed Fields**.

![WindowsTerminal\_tv0ar7PnGQ](https://github.com/user-attachments/assets/0fe3b08d-6b6e-4454-bbfe-6061cf1ae0b5)

#### **Avanço Manual (Step)**

Para inspecionar o programa detalhadamente, é possível avançar o PC manualmente.
Com a simulação pausada, pressione **'S'** para pular para a próxima instrução.

![WindowsTerminal\_0kLnfl2qIZ](https://github.com/user-attachments/assets/cc246d26-2d22-46b5-82c1-af72e73a3148)


## **5. Executando IO**
O FALCON-ASM foi projetado para ser intuitivo e direto, sem perder a fidelidade ao comportamento real da arquitetura RISC-V.
Para reduzir o boilerplate e tornar o código mais legível, a plataforma inclui um conjunto de pseudo-instruções que simplificam operações comuns - especialmente de entrada e saída (I/O) e de manipulação da pilha (STACK).

Embora todas as pseudo-instruções estejam documentadas em format.md, este capítulo apresenta um exemplo prático que demonstra como o FALCON interage com o I/O e como essas abstrações facilitam o desenvolvimento.

O código abaixo ilustra o uso das pseudo-instruções de I/O (printStr, readWord, print, etc.) para criar um pequeno programa que lê dois números, soma e exibe o resultado formatado.
```
.data 
 start: .asciz "Enter two numbers"
 theSum: .asciz "The sum of "
 with: .asciz " with "
 is: .asciz " is "
 void: .asciz ""
number1: .word 0
number2: .word 0

.text 
printStrLn start
readWord number1
readWord number2
printStrLn void
call calculate

printStr theSum
print s1
printStr with
print s2
printStr is
print s0

halt


calculate:
    la t0, number1
    lw t1,0(t0)
    la t2,number2
    lw t3,0(t2)
    add s0,t1,t3
    mv s1,t1
    mv s2,t3
    ret

```

A execução está logo abaixo

![WindowsTerminal_e0XEHNKWAu](https://github.com/user-attachments/assets/f9b87e26-bc89-4a14-88cb-4b46bf8aed64)
