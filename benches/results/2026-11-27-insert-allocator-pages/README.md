# Память первого INSERT на 100k

Релизный движок fe2717127, Linux x64 со статическим TCMalloc. Измерительный worker c07e5de55 читает счётчики TCMalloc и /proc/self/smaps_rollup после подготовки запроса и после коммита, за пределами таймера. Каждая строка — медиана изменения за INSERT по четырём независимым процессам. В каждом процессе три INSERT в отдельные свежие базы. Результат каждого INSERT проверен по всем строкам. RSS включает память всего процесса.

| CPU | Серия | INSERT | Объекты, МиБ | Куча, МиБ | RSS, МиБ | AnonHugePages, МиБ |
|---|---|---|---:|---:|---:|---:|
| AMD EPYC 7763 64-Core Processor | memory-1 | 1 | +398.3 | +418.5 | +411.8 | +0.0 |
| AMD EPYC 7763 64-Core Processor | memory-1 | 2 | +397.7 | +0.0 | +7.0 | +0.0 |
| AMD EPYC 7763 64-Core Processor | memory-1 | 3 | +397.7 | +0.0 | +0.9 | +0.0 |
| AMD EPYC 9V74 80-Core Processor | memory-2 | 1 | +398.3 | +418.5 | +411.8 | +0.0 |
| AMD EPYC 9V74 80-Core Processor | memory-2 | 2 | +397.7 | +0.0 | +7.0 | +0.0 |
| AMD EPYC 9V74 80-Core Processor | memory-2 | 3 | +397.7 | +0.0 | +0.6 | +0.0 |
| AMD EPYC 9V74 80-Core Processor | memory-3 | 1 | +398.3 | +418.5 | +412.6 | +0.0 |
| AMD EPYC 9V74 80-Core Processor | memory-3 | 2 | +397.7 | +0.0 | +7.1 | +0.0 |
| AMD EPYC 9V74 80-Core Processor | memory-3 | 3 | +397.7 | +0.0 | +1.4 | +0.0 |

Первый INSERT увеличивает кучу на 418,5 МиБ, повторные обходятся уже выделенной кучей при почти том же приросте живых объектов. Эти значения характеризуют всю операцию и сами по себе не указывают, какие структуры занимают память или сколько миллисекунд занимает рост кучи.

[Прогон и исходные снимки](https://github.com/themoretheless/lin/actions/runs/46009756148)

## Разбивка по этапам

Для каждого из четырёх независимых процессов суммируется время десяти пакетов по 10k. Последний столбец показывает медиану разности между первым INSERT и медианой следующих двух в том же процессе. Счётчики памяти TCMalloc сняты вне таймеров этапов.

| CPU | Этап | Первый INSERT, мс | Живые объекты, МиБ | Куча, МиБ | Первый минус повторные, мс |
|---|---|---:|---:|---:|---:|
| AMD EPYC 9V45 96-Core Processor | build | 56.8 | 370.09 | 362.15 | 24.42 |
| AMD EPYC 9V45 96-Core Processor | validate | 7.79 | 0.55 | 1.0 | 0.01 |
| AMD EPYC 9V45 96-Core Processor | maintain | 42.88 | 39.12 | 55.36 | 8.33 |
| AMD EPYC 9V45 96-Core Processor | append | 3.6 | -11.48 | 0.0 | 3.15 |
| AMD EPYC 7763 64-Core Processor | build | 103.09 | 370.09 | 362.15 | 63.92 |
| AMD EPYC 7763 64-Core Processor | validate | 8.71 | 0.55 | 1.0 | 0.62 |
| AMD EPYC 7763 64-Core Processor | maintain | 56.78 | 39.12 | 55.36 | 17.18 |
| AMD EPYC 7763 64-Core Processor | append | 7.51 | -11.48 | 0.0 | 6.93 |
| INTEL(R) XEON(R) PLATINUM 8573C | build | 85.77 | 370.09 | 362.15 | 45.53 |
| INTEL(R) XEON(R) PLATINUM 8573C | validate | 8.72 | 0.55 | 1.0 | 0.35 |
| INTEL(R) XEON(R) PLATINUM 8573C | maintain | 55.18 | 39.12 | 55.36 | 16.82 |
| INTEL(R) XEON(R) PLATINUM 8573C | append | 5.56 | -11.48 | 0.0 | 4.38 |

[Исходные измерения по этапам](https://github.com/themoretheless/lin/actions/runs/46010572924)
