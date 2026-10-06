# Psychology and HCI evidence for email triage, notification and AI-assisted decision tools

Research memo for Riddle, compiled 2026-09-25 from fetched primary sources (author PDFs, ACM DL / arXiv /
OpenAlex / Semantic Scholar records, NN/g, Laws of UX). Each entry gives study, authors, year, venue, method,
key finding, effect size where the source reports one, and a URL. Anything not confirmed against a fetched
source is marked **[unverified]**. Quotations are kept under 15 words.

Conventions: *Method* is what the study did; *Effect* is the reported statistic or "not reported"; *Strength*
is my reading of the evidence (strong / moderate / contested / argument-only).

---

## A. Email behaviour

### A1. Whittaker & Sidner 1996 — "Email overload"
- CHI '96, pp. 276–283. https://doi.org/10.1145/238386.238530
- Method: quantitative analysis of 20 Lotus users' mail archives, plus interviews.
- Finding: email, designed for asynchronous communication, had been co-opted for task management and personal
  archiving; inboxes were cluttered with hundreds of items mixing open tasks, half-read documents and threads.
  Three management strategies: **no filers**, **frequent filers**, **spring cleaners**.
- Effect: descriptive, no inferential statistics. Exact inbox-size means are in the paywalled PDF and were not
  re-verified **[unverified]**. Strength: strong qualitative taxonomy; the field's founding paper.

### A2. Fisher, Brush, Gleave & Smith 2006 — "Revisiting Whittaker & Sidner"
- CSCW '06. https://doi.org/10.1145/1180875.1180922
- Method: 600 mailboxes at a high-tech company compared with 1996.
- Finding: inboxes roughly the same size as in 1996 but archives ten times larger; little evidence of distinct
  strategies, most users in a "middle ground". Effect: descriptive. Strength: moderate (single company).

### A3. Dabbish, Kraut, Fussell & Kiesler 2005 — "Understanding email use"
- CHI '05, pp. 691–700. https://doi.org/10.1145/1054972.1055068 ;
  PDF https://sfussell.hci.cornell.edu/pubs/Manuscripts/Dabbish_CHI2005.pdf
- Method: web survey, N=121 respondents rating 581 specific incoming messages.
- Finding: respondents deleted 24%, filed 27% and **left 49% in the inbox**; replied to about a third.
  Importance was driven by work-relatedness and whether action was required; importance had only a modest effect
  on replying and saving, social factors mattered too. Descriptives: 30 messages read/day (median 25), 19
  checks/day (median 13), median inbox 105 (mean 1,336, s.d. 2,785).
- Effect: regression models with modest coefficients. Strength: moderate.

### A4. Dabbish & Kraut 2006 — "Email overload at work"
- CSCW '06. https://doi.org/10.1145/1180875.1180941
- Method: large nationwide organisational survey; scale development for "email strain".
- Finding: higher volume was associated with more felt overload, **moderated by management strategy**.
- Effect: survey correlations; abstract gives no coefficients. Strength: moderate.

### A5. Bellotti, Ducheneaut, Howard & Smith 2003 — "Taking email to task"
- CHI '03, pp. 345–352. https://doi.org/10.1145/642611.642672
- Method: fieldwork on email as task management, then Taskmaster, a client embedding task resources
  ("thrasks" = thread-tasks), evaluated on live email over two weeks.
- Finding: email *is* the task manager for many workers; a task-centric client was valued by overloaded users.
  Effect: qualitative. Strength: moderate.

### A6. González & Mark 2004 — "Constant, constant, multi-tasking craziness"
- CHI '04, pp. 113–120. https://doi.org/10.1145/985692.985707 ; PDF https://ics.uci.edu/~gmark/CHI2004.pdf
- Method: field observation of analysts, developers and managers.
- Finding: ~3 minutes per task and ~2 minutes per tool before switching; ~10 "working spheres", ~12 minutes in
  one before switching. Effect: descriptive means. Strength: strong descriptive baseline.

### A7. Mark, Voida & Cardello 2012 — "A pace not dictated by electrons"
- CHI '12. https://doi.org/10.1145/2207676.2207754 ;
  PDF https://ics.uci.edu/~gmark/Home_page/Publications_files/CHI%202012.pdf
- Method: 13 information workers, email cut off for five workdays; window-switch logging, wearable heart-rate
  monitors, ethnography; single site.
- Finding: without email, **window switches fell from 37.1 to 18.2 per hour** and mean window dwell rose from
  75.5 s to 131.9 s; heart-rate variability indicated lower stress (mean difference t(6)=−2.26, p=.065; Levene's
  test on variance F(1,40409)=170.9, p<.0001). Participants reported better focus.
- Strength: moderate (N=13, one site, marginal test at the means level).

### A8. Mark, Iqbal, Czerwinski, Johns & Sano 2016 — "Email duration, batching and self-interruption"
- CHI '16, pp. 1717–1728. https://doi.org/10.1145/2858036.2858262 ;
  PDF https://ics.uci.edu/~gmark/Home_page/Publications_files/CHI%2016%20Email%20Duration.pdf
- Method: 40 workers, 12 workdays, logging + biosensors + daily surveys, in situ.
- Finding: ~1.5 h/day on email, **77 checks/day** on average (range 1–408); checks correlated with duration,
  r=.75. Longer daily email time predicted lower rated productivity and higher measured stress. Self-interrupting
  checkers (41%) rated productivity higher with long email time than notification-driven checkers (31%). Batching
  was associated with higher rated productivity but **no evidence that batching lowers stress**.
- Strength: strong in-situ study; correlational.

### A9. Aberdeen, Pacovsky & Slater 2010 — "The learning behind Gmail Priority Inbox"
- LCCC: NIPS 2010 Workshop. https://research.google/pubs/pub36955
- Method: engineering note. Importance = probability the user acts on a message (open, reply, manual correction)
  within a time window; per-user logistic models updated near-online; social, content, thread and label
  features; manual corrections are training signal.
- Finding: among ~2,000 Googlers with similar volumes, Priority Inbox users spent **6% less time reading mail
  overall and 13% less reading unimportant mail**, and bulk-archived more confidently.
- Strength: moderate (internal, non-randomised).

### A10. Kalman et al. — response latency ("chronemics")
- Kalman, Ravid, Raban & Rafaeli 2006, *Pauses and response latencies*, JCMC 12(1).
  https://doi.org/10.1111/j.1083-6101.2006.00312.x — >150,000 responses (Enron email 15,815; course forums;
  Google Answers). Latencies follow a power law (slopes −1.74 to −2.04); **~80% of replies arrive within the
  group's mean latency and ~97% within 10× it**; silence = no reply after 10× the norm.
- Kalman & Rafaeli 2011, *Online pauses and silence*, Communication Research 38(1).
  https://doi.org/10.1177/0093650210378229 — managers rated candidates whose reply took 1 day, 2 weeks or >1
  month; delay is an expectancy violation whose effect is moderated by the sender's reward valence.
- Strength: strong descriptive (2006); moderate experimental (2011).

### A11. Inbox Zero (Merlin Mann) and whether it helps
- 2006 series on 43 Folders and a Google Tech Talk on 23 July 2007. https://en.wikipedia.org/wiki/Merlin_Mann ;
  talk https://www.youtube.com/watch?v=z9UjeTMb3Yk . In 2020 Mann said the term is misunderstood: "zero" was
  attention spent on the inbox, not message count (per the Wikipedia entry).
- Evidence: no peer-reviewed test of Inbox Zero itself was found. Nearest evidence: Kushlev & Dunn 2015 (fewer
  checks → lower stress, B5), Mark 2016 (no evidence batching lowers stress, A8), Fitz 2019 (3 batches/day helps,
  B4). Treat "empty inbox" as **argument-only**; "fewer, deliberate checks" as moderately supported.

### A12. Cecchinato, Cox & Bird 2015 — "Working 9-5?"
- CHI '15. https://doi.org/10.1145/2702123.2702537
- Method: exploratory interviews with university staff across professions.
- Finding: permeable boundaries (mostly academics) vs rigid boundaries (mostly professional services); people use
  "micro-boundary" strategies (separate accounts, devices, notification rules). Strength: qualitative, moderate.

### A13. Newport 2021 — *A World Without Email* (argument, not study)
- Penguin Random House. https://www.calnewport.com/books/a-world-without-email/
- Argument: knowledge work runs on a "hyperactive hive mind" of unscheduled messages; context switching is
  expensive (leaning on Gloria Mark's work); replace ad hoc messaging with explicit processes; treat attention as
  capital. Strength: argument-only, but its empirical planks (A6, A7, B1) are real.

---

## B. Attention and interruption

### B1. Mark, Gudith & Klocke 2008 — "The cost of interrupted work"
- CHI '08, pp. 107–110. https://doi.org/10.1145/1357054.1357072 ; PDF https://ics.uci.edu/~gmark/chi08-mark.pdf
- Method: lab experiment, 48 participants, email-answering task interrupted every ~2 min by phone or IM in
  same/different context; NASA-TLX plus a stress item.
- Finding: context did not matter; interrupted people finished *faster* (22.8 min baseline vs 20.3/20.6 min,
  p<.05) with no quality loss, but reported higher workload, stress, frustration, time pressure and effort
  (workload F(2,92)=3.82, p<.03). Openness and need-for-structure predicted disruption cost (β=−.36, −.35).
- Note: the popular "23 minutes to recover" figure is *not* in this paper. Strength: strong lab evidence.

### B2. Iqbal & Horvitz 2007 — "Disruption and recovery of computing tasks"
- CHI '07, pp. 677–686. https://doi.org/10.1145/1240624.1240730 ;
  PDF https://www.microsoft.com/en-us/research/wp-content/uploads/2016/11/CHI_2007_Iqbal_Horvitz-1.pdf
- Method: field logging of 27 Microsoft workers for two weeks (2,267 h).
- Finding: ~4.3 email and ~3.2 IM alerts per hour; after an email alert users took ~5 min to switch; ~10 min on
  alert-caused switches and another 10–15 min before refocusing; **27% of suspensions took >2 h to resume**.
  Visible windows of suspended apps sped resumption. Strength: strong descriptive field data.

### B3. Pielot, Church & de Oliveira 2014 — "An in-situ study of mobile phone notifications"
- MobileHCI '14 (best paper). https://doi.org/10.1145/2628363.2628364
- Method: 15 users, one week of logging plus daily diaries (6,854 notifications).
- Finding: median **63.5 notifications/day**, mostly messengers (48.7%) and email (32.4%); attended within
  minutes (messengers 3.5–6.6 min; email up to 27.7 min median at weekends); more notifications → more feeling
  overwhelmed, interrupted, stressed; social ones → more connected. Authors advise *not* simply cutting
  notifications (people check more) but timing email alerts and communicating availability.
- Strength: moderate (small N, rich data).

### B4. Fitz, Kushlev, Jagannathan, Lewis, Paliwal & Ariely 2019 — "Batching smartphone notifications"
- Computers in Human Behavior 101:84–94. https://doi.org/10.1016/j.chb.2019.07.016
- Method: randomised field experiment, N=237 (India): as usual, hourly batches, three batches/day, none; daily
  diaries plus app logs; powered for Cohen's f≈0.22.
- Finding: **three batches/day** → more attentive, productive, better mood, more control, lower stress; **hourly ≈
  control**; **none** → more anxiety and fear of missing out. Mediated by reduced inattention.
- Strength: strong (RCT), one culture.

### B5. Kushlev & Dunn 2015 — "Checking email less frequently reduces stress"
- Computers in Human Behavior 43:220–228. https://doi.org/10.1016/j.chb.2014.11.005 ;
  PDF https://www.interruptions.net/literature/Kushlev-ComputHumBehav15.pdf
- Method: within-subjects, 124 adults; one week limited to three checks/day vs one week unlimited.
- Finding: checks fell from 12.5 to 4.7/day (t(115)=10.2, p<.001) with the same volume received and answered;
  daily stress was significantly lower in the limited week; lower stress predicted better well-being.
- Strength: strong (experimental), self-report stress.

---

## C. Decision and cognition laws

### C1. Hick–Hyman law
- Hick 1952, QJEP 4:11–26; Hyman 1953, JEP 45:188–196. https://doi.org/10.1037/h0056940 ; https://lawsofux.com/hicks-law/
- Choice reaction time rises with the log of the number of equiprobable alternatives. Applies to simple,
  well-learned choices; familiar menus often escape it. Strength: strong classic, narrow scope.

### C2. Fitts's law
- Fitts 1954, JEP 47(6):381–391. https://doi.org/10.1037/h0055392 ; MacKenzie 1992, HCI 7(1):91–139.
  https://doi.org/10.1207/s15327051hci0701_3 ; https://lawsofux.com/fittss-law/
- Movement time is a log function of distance/size; screen edges are infinitely deep targets. Strength: strong.

### C3. Miller 1956 and Cowan 2001
- Miller, Psychological Review 63:81–97. https://doi.org/10.1037/h0043158 ; Cowan, BBS 24:87–114.
  https://doi.org/10.1017/S0140525X01003922
- Miller's 7±2 was a rhetorical estimate; Cowan's review puts working-memory capacity at **3–5 chunks** when
  rehearsal and chunking are blocked. Strength: strong. Reading: keep simultaneous unchunked decisions ≤4.

### C4. Cognitive load theory — Sweller 1988
- Cognitive Science 12(2):257–285. https://doi.org/10.1207/s15516709cog1202_4
- Learning suffers when material exceeds working memory; later split into intrinsic, extraneous and germane load.
  Strength: strong in instruction; extrapolation to UI is by analogy (moderate).

### C5. Progressive disclosure — NN/g
- Nielsen 2006. https://www.nngroup.com/articles/progressive-disclosure/
- Show only frequent options first to improve learnability, efficiency and error rate; keep to **two levels**;
  pick the initial set by task frequency. Evidence is NN/g usability testing (46 web apps), not a controlled
  experiment. Strength: moderate (practitioner).

### C6. Zeigarnik effect — replication status
- Ghibellini & Meier 2025, meta-analysis, Humanities and Social Sciences Communications.
  https://doi.org/10.1057/s41599-025-05000-w
- Method: 59 publications (38 Zeigarnik, 20 Ovsiankina, 1 both) from 1,455 screened.
- Finding: **no memory advantage** for unfinished tasks (weighted ratio interrupted:completed recalled = 0.99;
  d_z=0.15 across 8 studies) but a robust **tendency to resume** interrupted tasks (weighted resumption 66.8%).
- Strength: the memory claim is refuted; the resumption pull (Ovsiankina) is moderate.

### C7. Decision fatigue — Baumeister, Danziger, and the critiques
- Danziger, Levav & Avnaim-Pesso 2011, PNAS 108(17). https://doi.org/10.1073/pnas.1018033108 — 1,112 Israeli
  parole rulings; favourable rulings fell from ~65% to near 0 within a session, rebounding after food breaks.
- Weinshall-Margel & Shapard 2011, PNAS letter. https://doi.org/10.1073/pnas.1110910108 — case ordering (prisons
  seen before breaks; unrepresented prisoners last) explains much of it; panels, not lone judges.
- Glöckner 2016, JDM 11(6):601–610. http://journal.sjdm.org/16/16823/jdm16823.pdf — simulation: a rational
  judge who avoids starting long (favourable) cases before a break reproduces the curve; magnitude overestimated.
- Hagger et al. 2016, Perspectives on Psychological Science 11(4). https://doi.org/10.1177/1745691616652873 —
  23 labs, 2,141 participants, pooled **d≈0.04**; no evidence for ego depletion.
- Strength: **contested**. Do not build on "decision fatigue" as a mechanism; do build on the uncontroversial point
  that long unstructured queues degrade attention (B1, B2).

### C8. Choice overload — Iyengar & Lepper 2000 vs Scheibehenne et al. 2010
- Iyengar & Lepper, JPSP 79(6):995–1006. https://doi.org/10.1037/0022-3514.79.6.995 — Study 1 (jam booth): 60%
  stopped at 24 jams vs 40% at 6, but **30% bought after 6 vs 3% after 24** (χ²(1,N=249)=32.3, p<.0001);
  Studies 2–3 replicated with essays and chocolates.
- Scheibehenne, Greifeneder & Todd 2010, JCR 37(3):409–425. https://doi.org/10.1086/651235 — meta-analysis of 63
  conditions, 50 experiments, N=5,036: **mean effect ≈ 0** with large heterogeneity; preconditions (no dominant
  option, no prior preference, complex options) matter; no sufficient condition found.
- Strength: contested as a general law; moderate as a conditional effect.

### C9. Goal-gradient and endowed progress — Nunes & Drèze 2006
- JCR 32(4):504–512. https://doi.org/10.1086/500480
- Field experiment with car-wash loyalty cards: 8 stamps needed vs 10 with 2 pre-stamped; redemption **34% vs
  19%**, faster completion; moderated by the reason given and the progress "currency". Goal-gradient origin: Hull
  1932; Kivetz, Urminsky & Zheng 2006 (https://lawsofux.com/goal-gradient-effect/). Strength: moderate.

### C10. Peak–end rule and duration neglect
- Kahneman, Fredrickson, Schreiber & Redelmeier 1993, Psychological Science 4(6).
  https://doi.org/10.1111/j.1467-9280.1993.tb00589.x — cold-water trials: a slightly warmer 30 s tail made the
  longer trial preferred. Fredrickson & Kahneman 1993, JPSP 65(1):45. https://doi.org/10.1037/0022-3514.65.1.45
- Redelmeier & Kahneman 1996, *Pain* 66:3–8. https://doi.org/10.1016/0304-3959(96)02994-6 — colonoscopy (n=154)
  and lithotripsy (n=133): remembered pain tracked peak and last-3-minute intensity (both p<.005); duration barely
  mattered.
- Alaybek et al. 2022 meta-analysis, OBHDP 170. https://doi.org/10.1016/j.obhdp.2022.104149 — abstract not
  retrieved; reported to support peak and end while finding duration is not fully neglected **[unverified]**.
- Strength: strong for peak/end; duration neglect moderate.

### C11. Response-time thresholds — Nielsen and the Doherty threshold
- Nielsen 1993 (from *Usability Engineering*; sources Miller 1968, Card et al. 1991).
  https://www.nngroup.com/articles/response-times-3-important-limits/ — **0.1 s** feels instantaneous; **1 s**
  keeps flow of thought; **10 s** is the attention limit, beyond which show percent-done.
- Doherty & Thadani 1982, IBM Systems Journal 21(1), reproduced at
  https://jlelliotton.blogspot.com/p/the-economic-value-of-rapid-response.html — productivity rose more than
  proportionally as response fell below 1 s; 0.3 s gave 371 transactions/h vs 180 at 3 s. The "400 ms" framing
  is the popular gloss (https://lawsofux.com/doherty-threshold/); the article says sub-second. Sample: 1980s
  engineers on mainframes. Strength: thresholds strong as rules of thumb; Doherty's numbers dated.

### C12. Jakob's law
- https://lawsofux.com/jakobs-law/ — users spend most time on other products and expect yours to work the same
  way. Strength: heuristic (moderate), consistent with transfer-of-learning research (F1).

### C13. Tesler's law (conservation of complexity)
- Larry Tesler, Xerox PARC, mid-1980s. https://lawsofux.com/teslers-law/ — every process has irreducible
  complexity; the question is who carries it, system or user. Strength: design axiom (argument-only).

### C14. Von Restorff (isolation) effect
- von Restorff 1933. https://lawsofux.com/von-restorff-effect/ — the item that differs from its neighbours is
  remembered best. Strength: strong in memory research; the UI corollary (highlight one thing) moderate.

### C15. Serial position effect
- Murdock 1962, JEP 64(5):482–488. https://doi.org/10.1037/h0045106 — steep primacy over the first 3–4 items,
  S-shaped recency over the last ~8. https://lawsofux.com/serial-position-effect/ Strength: strong.

---

## D. Trust in automation and AI

### D1. Lee & See 2004 — "Trust in automation: designing for appropriate reliance"
- Human Factors 46(1):50–80. https://doi.org/10.1518/hfes.46.1.50_30392
- Method: integrative review. Finding: trust guides reliance when automation is too complex to understand; the
  goal is **appropriate** trust — calibration (trust matches capability), resolution (trust tracks changes in
  capability) and specificity (trust attaches to the right function/context). These three terms are from the
  paper body, confirmed only via abstract and secondary sources **[partly unverified]**. Design advice: display
  the automation's purpose, process and performance; let trust evolve with observed reliability.
- Strength: strong conceptual foundation (6,000+ citations).

### D2. Parasuraman & Riley 1997 — use, misuse, disuse, abuse
- Human Factors 39(2):230–253. https://doi.org/10.1518/001872097778543886
- **Misuse** = over-reliance (monitoring failures, decision biases); **disuse** = under-use, commonly caused by
  false alarms when base rates are ignored; **abuse** = automating without regard to the human. Strength: strong.

### D3. Parasuraman, Sheridan & Wickens 2000 — types and levels of automation
- IEEE Trans. SMC-A 30(3):286–297. https://doi.org/10.1109/3468.844354
- Four stages — information acquisition, information analysis, decision selection, action implementation — each
  at levels 1 (manual) to 10 (full autonomy). Automating *decision and action* carries the largest risk to
  situation awareness and skill. Strength: strong framework.

### D4. Automation bias — Mosier & Skitka
- Mosier, Skitka, Heers & Burdick 1998, Int. J. Aviation Psychology 8(1):47–63.
  https://doi.org/10.1207/s15327108ijap0801_3 — glass-cockpit pilots; **omission errors** (missing events the
  aid did not flag) and **commission errors** (following a wrong aid despite contrary evidence); accountability
  manipulations did not help significantly.
- Skitka, Mosier & Burdick 1999, IJHCS 51(5):991–1006. https://doi.org/10.1006/ijhc.1999.0252 — students in a
  simulated flight task; those *without* an aid outperformed those with a highly-but-not-perfectly reliable aid on
  monitoring (record summary; full numbers not retrieved).
- Goddard, Roudsari & Wyatt 2012, JAMIA 19(1). https://doi.org/10.1136/amiajnl-2011-000089 — systematic review
  of frequency, mediators and mitigators in clinical decision support.
- Strength: strong, replicated across aviation and medicine.

### D5. Algorithm aversion — Dietvorst, Simmons & Massey 2015
- JEP: General 144(1):114–126. https://doi.org/10.1037/xge0000033
- Method: 5 incentivised forecasting studies (MBA admissions, state data); lab N≈361, 206, 354 plus MTurk.
- Finding: people who **saw the algorithm err** were less confident in it and less likely to choose it over an
  inferior human, *even after seeing it outperform the human*; confidence in algorithms drops faster than in
  humans after the same error. Strength: strong, replicated.

### D6. Overcoming algorithm aversion — Dietvorst, Simmons & Massey 2018
- Management Science 64(3):1155–1170. https://doi.org/10.1287/mnsc.2016.2643
- Method: 3 studies (Study 1 N=288); can't-change, adjust-by-10 percentiles, change-10-of-20, use-freely.
- Finding: people were considerably more likely to use an imperfect algorithm when they could **modify its output
  even slightly**, and forecast better as a result; preference was insensitive to how much modification was
  allowed; modifiability raised satisfaction and future use. Strength: strong; the mechanism is a desire for *some*
  control.

### D7. Amershi et al. 2019 — "Guidelines for Human-AI Interaction" (the 18)
- CHI '19. https://doi.org/10.1145/3290605.3300233 ;
  PDF https://www.microsoft.com/en-us/research/wp-content/uploads/2019/01/Guidelines-for-Human-AI-Interaction-camera-ready.pdf
- Method: synthesis of >150 recommendations, modified heuristic evaluation, and a study with 49 practitioners
  applying the guidelines to 20 AI products.
- Initially: **G1** Make clear what the system can do. **G2** Make clear how well the system can do what it can do.
- During interaction: **G3** Time services based on context. **G4** Show contextually relevant information.
  **G5** Match relevant social norms. **G6** Mitigate social biases.
- When wrong: **G7** Support efficient invocation. **G8** Support efficient dismissal. **G9** Support efficient
  correction. **G10** Scope services when in doubt. **G11** Make clear why the system did what it did.
- Over time: **G12** Remember recent interactions. **G13** Learn from user behavior. **G14** Update and adapt
  cautiously. **G15** Encourage granular feedback. **G16** Convey the consequences of user actions. **G17** Provide
  global controls. **G18** Notify users about changes.
- Strength: strong consensus document; validated for usability of the guidelines, not for outcomes.

### D8. Bansal et al. 2021 — "Does the whole exceed its parts?"
- CHI '21. https://doi.org/10.1145/3411764.3445717 ; arXiv https://arxiv.org/abs/2006.14779
- Method: crowd studies on three tasks (beer-review and book-review sentiment; LSAT logic) with an AI of
  *human-comparable* accuracy; explanation conditions incl. AI-generated, expert, and "adaptive" (explain top-1
  when confident, the alternative otherwise).
- Finding: AI recommendations produced complementary gains, but **explanations did not add accuracy; they raised
  acceptance of the AI's answer whether right or wrong**. Explanations helped when the AI was right, hurt when
  wrong, netting ~0 even for adaptive explanations. Prior "explanations help" results came from settings where the
  AI vastly outperformed humans (e.g. 87% vs 51%). Strength: strong, large N, three tasks.

### D9. Buçinca, Malaya & Gajos 2021 — cognitive forcing functions
- PACM HCI 5, CSCW1, Art. 188. https://doi.org/10.1145/3449287
- Method: N=199 MTurk; nutrition task; no-AI, simple XAI (explanation; "87% confident" uncertainty), and three
  CFFs — **on-demand** (AI shown only if asked), **update** (decide first, then see AI and may change), **wait**
  (AI shown after a delay).
- Finding: CFFs **significantly reduced overreliance** vs simple XAI, but users rated the most effective designs
  least favourably; benefits accrued mostly to people high in Need for Cognition. Strength: strong; the
  overreliance–satisfaction trade-off is the key point.

### D10. Zhang, Liao & Bellamy 2020 — confidence display and trust calibration
- FAT* '20. https://doi.org/10.1145/3351095.3372852 ; arXiv https://arxiv.org/abs/2001.02114
- Method: two experiments, income prediction (UCI Adult); human ≈65% vs AI ≈75%; manipulated showing
  prediction, confidence score, local explanation.
- Finding: **showing confidence calibrated trust** — people switched to the AI more when confidence was high and
  less when low (info factor F(2,24)=4.17, p=.028; interaction with confidence F(8,96)=3.81, p<.001); **local
  explanations did not**. Calibration alone did not raise joint accuracy because humans lacked complementary
  knowledge on this task. Strength: strong for the calibration claim; one task.

### D11. Uncertainty display and "appropriate trust" (supporting studies)
- Prabhudesai, Yang, Asthana, Huan & Liao 2023, IUI '23. https://doi.org/10.1145/3581641.3584033 — think-aloud
  with 17 lay decision-makers: uncertainty displays made people slow down and think, reducing over-reliance.
- Vodrahalli, Gerstenberg & Zou 2022, NeurIPS — showing the AI as *more* confident than it is improved joint
  accuracy in their setting. **Contested**: optimises outcome at the cost of honest calibration.
- Ma et al. 2023, CHI '23. https://doi.org/10.1145/3544548.3581058 — calibrating on both human and AI
  correctness likelihood per instance promoted appropriate trust.
- Strength: moderate, converging: instance-level confidence helps; explanations alone do not.

### D12. Kulesza, Burnett, Wong & Stumpf 2015 — "Principles of explanatory debugging" (in depth)
- IUI '15, pp. 126–137. https://doi.org/10.1145/2678025.2701399 ;
  PDF https://openaccess.city.ac.uk/id/eprint/13819/1/paper326.pdf
- Problem: a user ("Alice") wants her mail classifier to learn a concept she can state in words, but has too few
  examples and cannot see why it is wrong. Black-box instance labelling is slow and opaque.
- Approach: **Explanatory Debugging** — the system explains each prediction; the user explains corrections back;
  the loop is iterative.
- Explainability principles: **1.1 Be sound** (never simplify the model into a lie); **1.2 Be complete** (show all
  reasoning that matters — here, all features); **1.3 But don't overwhelm** (bounded feature sets; 10
  highest-information-gain words performed as well as 50).
- Correctability principles: **2.1 Be actionable** (every explanation element is editable); **2.2 Be reversible**
  (users make things worse sometimes; undo is required); **2.3 Always honor user feedback** (ignored feedback stops
  people giving it); **2.4 Incremental changes matter** (small, visible effects keep the mental model coherent; avoid
  retraining that changes everything).
- Prototype EluciDebug: multinomial naïve Bayes over 20 Newsgroups (rec.sport.hockey vs rec.sport.baseball);
  explanations show the words used, each word's per-topic weight as paired bars, folder-size priors and a "why"
  panel; users add/remove words and drag bars to change weights; every change re-predicts immediately and shows
  the delta.
- Evaluation: 77 participants (27 F, 50 M) with no ML background; treatment vs a black-box control that could only
  label messages.
- Findings: treatment mental-model scores **52% higher** (W=259, p<.001), largely from naming the features; they
  corrected the classifier **up to twice as efficiently** per action; feature feedback reached targets with far
  fewer labelled messages.
- Strength: strong for the mechanism (single lab study). The closest published analogue to a user editing Riddle's
  rules and signals.

### D13. Stumpf et al. and Kulesza et al. — end-user interaction with email classifiers
- Stumpf, Rajaram, Li, Burnett, Dietterich, Sullivan, Drummond & Herlocker 2007, IUI '07.
  https://web.engr.oregonstate.edu/~burnett/Reprints/iui07-mlexplanations.pdf — think-aloud with 13 email users;
  classifiers sorted mail into folders with **rule-based, keyword-based or similarity-based** explanations. Users
  gave rich feedback freely (reweight, add or combine features, change the algorithm); rule- and keyword-based
  explanations were understood by nearly all, similarity-based by only 4 of 13.
- Stumpf et al. 2009, IJHCS 67(8). https://doi.org/10.1016/j.ijhcs.2009.03.004 — such feedback could be
  assimilated by learning algorithms and improved accuracy.
- Kulesza, Stumpf, Wong, Burnett, Perona, Ko & Oberst 2011, ACM TiiS 1(1). https://doi.org/10.1145/2030365.2030367
  (early version IUI '09; Kulesza et al. 2010 VLHCC) — users ask "why" questions and edit the answers; the dominant
  information need (42% of requests) was **what will happen if I change this weight**.
- Strength: moderate–strong; small-N qualitative plus algorithmic experiments.

### D14. Vaccaro, Almaatouq & Malone 2024 — meta-analysis of human–AI combinations
- Nature Human Behaviour 8:2293–2303. https://doi.org/10.1038/s41562-024-02024-1 ; https://arxiv.org/abs/2405.06087
- Method: preregistered meta-analysis of 106 experiments (2020–2023), 370 effect sizes.
- Finding: on average the team was **worse than the better of the two alone** (Hedges' g=−0.23, 95% CI −0.39 to
  −0.07); losses concentrated in **decision tasks** (g=−0.27), gains in **creation tasks** (g=+0.19, n.s.);
  combinations helped when the human was the stronger party and hurt when the AI was. Strength: strong.

### D15. Shneiderman 2020/2022 — Human-Centered AI (reliable, safe, trustworthy)
- IJHCI 36(6):495–504. https://doi.org/10.1080/10447318.2020.1741118 ; https://arxiv.org/abs/2002.04087 ;
  book *Human-Centered AI*, OUP 2022.
- Replace the one-dimensional Sheridan–Verplank autonomy scale with a **two-dimensional** space (human control ×
  computer automation) and aim high on both; RST = reliable (engineering practices, **audit trails / "flight data
  recorders"**), safe (safety culture), trustworthy (independent oversight). Strength: framework, widely adopted.

### D16. Norman 2005 — "Human-centered design considered harmful"
- ACM interactions 12(4):14–19. https://doi.org/10.1145/1070960.1070976 ;
  https://jnd.org/human-centered-design-considered-harmful/
- Design around the *activity*, not individual requests; some tools legitimately require learning; excessive
  responsiveness to feedback produces incoherent designs. Strength: argument-only.

---

## E. Forgiveness and control

### E1. Shneiderman's Eight Golden Rules
- *Designing the User Interface*, 6th ed. (Pearson 2016). https://www.cs.umd.edu/users/ben/goldenrules.html
- 1. Strive for consistency. 2. Seek universal usability. 3. Offer informative feedback. 4. Design dialogs to
  yield closure. 5. Prevent errors. 6. Permit easy reversal of actions. 7. Keep users in control. 8. Reduce
  short-term memory load. Rule 6 is stated with its rationale: reversibility relieves anxiety and encourages
  exploration. Strength: expert heuristics.

### E2. Nielsen's 10 usability heuristics
- Nielsen 1994 (refined by factor analysis of 249 usability problems; Molich & Nielsen 1990).
  https://www.nngroup.com/articles/ten-usability-heuristics/
- 1. Visibility of system status. 2. Match between system and the real world. 3. User control and freedom.
  4. Consistency and standards. 5. Error prevention. 6. Recognition rather than recall. 7. Flexibility and
  efficiency of use. 8. Aesthetic and minimalist design. 9. Help users recognize, diagnose, and recover from errors.
  10. Help and documentation. Strength: expert heuristics derived from problem catalogues.

### E3. Norman — gulfs and the seven stages of action
- *The Design of Everyday Things* (1988; revised 2013). https://en.wikipedia.org/wiki/The_Design_of_Everyday_Things
- Goal → plan → specify → perform (the **gulf of execution**) → perceive → interpret → compare (the **gulf of
  evaluation**). Good design shrinks both: obvious how to act, obvious what happened. Strength: conceptual model.

### E4. Abowd & Dix 1992 — "Giving undo attention"
- Interacting with Computers 4(3):317–342. https://doi.org/10.1016/0953-5438(92)90021-7 ;
  https://www.alandix.com/academic/papers/undo92/undo92.html
- Formal models of undo joined with how users *understand* undo; reframes undo as "a user intention to be
  supported", not a command; group-undo issues illuminate single-user cases. Strength: theoretical, foundational.

### E5. Evidence that reversibility lowers anxiety and increases exploration
- Direct controlled evidence is thin. Convergent lines: Shneiderman's rule 6 states it as the rationale (E1);
  Kulesza 2015 principle 2.2 found users *needed* reversal because edits sometimes made things worse (D12);
  Dietvorst 2018 shows slight control restores use of an imperfect system (D6); Nielsen 2018 recommends undo over
  confirmation because it lowers anxiety (E6). Treat "undo increases exploration" as **moderately supported by
  convergent sources, not by a single controlled study** [partly unverified].
- Gmail "Undo Send": launched in Gmail Labs on 19 March 2009
  (https://gmail.googleblog.com/2009/03/new-in-labs-undo-send.html) and made an official setting on 23 June 2015
  with a 5–30 s window (https://time.com/3932586/google-gmail-undo-send/). It works by *delaying* the send, turning
  an irreversible action into a reversible one for a grace period — a pattern, not a study.

### E6. Confirmation-dialog blindness
- Nielsen 2018, NN/g. https://www.nngroup.com/articles/confirmation-dialog/
- Confirmations work only for rare, serious, irreversible actions; overuse breeds habituation and automatic "Yes";
  generic "Are you sure?" gives no protection; buttons should name the action; **prefer undo**. Evidence is
  practitioner observation, with mechanistic backing from alarm-driven disuse (D2). Strength: moderate.

### E7. Levels of automation and control (cross-reference)
- D3's stages × levels and D15's two-dimensional model are the theory behind an autonomy ladder; D2 warns that
  each rung fails differently (disuse at the bottom, misuse at the top).

---

## F. Learnability, jargon and metaphor

### F1. Carroll & Thomas 1982 — "Metaphor and the cognitive representation of computing systems"
- IEEE Trans. SMC 12(2):107–116. https://doi.org/10.1109/TSMC.1982.4308795
- Learners extend old structures metaphorically; the metaphors users spontaneously generate predict how easily they
  learn; interfaces that suggest **inefficacious metaphors are harder to learn**. Strength: strong theory with
  early experiments.

### F2. Gentner & Nielsen 1996 — "The Anti-Mac interface" (metaphor limits)
- CACM 39(8):70–82. https://doi.org/10.1145/232014.232032 ; https://www.nngroup.com/articles/anti-mac-interface/
- Metaphors fail three ways — target lacks source features, source has features the target lacks, shared features
  behave differently — and cap what designers can invent. Nielsen's heuristic 2 (E2) and NN/g's "Match between
  system and real world" (Kaley 2018, https://www.nngroup.com/articles/match-system-real-world/): use the users'
  words, not system-oriented terms. Strength: argument plus heuristic.

### F3. Jargon and readability
- Oppenheimer 2006, Applied Cognitive Psychology 20:139–156. https://doi.org/10.1002/acp.1178 — 5 experiments:
  needless complexity lowered judged intelligence of the author, mediated by processing fluency.
- Shulman, Dixon, Bullock & Colón Amill 2020, J. Language & Social Psychology 39(5–6).
  https://doi.org/10.1177/0261927X20902177 — N=650; jargon disrupted fluency **even when definitions were
  provided**, reducing perceived understanding and interest. Strength: strong (experimental).

### F4. Current vs target knowledge — Spool 2005
- UIE, 10 Jan 2005. https://articles.centercentre.com/design_intuitive/
- Users arrive at a **current knowledge point** and need a **target knowledge point**; the gap is the design
  problem. A design feels intuitive when the gap is zero or the interface trains the user across it unnoticed.
  Strength: practitioner model.

### F5. Onboarding: tours vs contextual help — NN/g
- Laubheimer 2023. https://www.nngroup.com/articles/onboarding-tutorials/
- Users skip tutorials because they want to start (the "paradox of the active user", Carroll & Rosson 1987); tours
  do not improve task performance and are poorly retained because they arrive out of context; prefer **pull**
  revelations at the moment of need, dismissable and recallable, with progressive disclosure and no explanation of
  conventional features. Strength: moderate (NN/g research, not a published RCT).

### F6. The labour illusion — Buell & Norton 2011
- Management Science 57(9):1564–1579. https://doi.org/10.1287/mnsc.1110.1376
- 5 experiments (online travel, dating): when a site visibly shows the work it is doing, people can **prefer a
  longer wait** to an instant identical result; mediated by perceived effort and reciprocity; bounded by wait
  length and outcome quality. Strength: strong.

### F7. The IKEA effect — Norton, Mochon & Ariely 2012
- Journal of Consumer Psychology 22(3):453–460. https://doi.org/10.1016/j.jcps.2011.08.002
- 4 studies (IKEA boxes, origami, Lego): people value their own amateur creations near expert level; the effect
  **requires successful completion** — destroyed or unfinished work loses it; holds for novices and DIYers.
  Strength: strong.

### F8. Norman 2005 (see D16) — some tools warrant learning; design the activity.

---

## G. Habit and ritual

### G1. Fogg Behavior Model 2009
- Persuasive '09. https://doi.org/10.1145/1541948.1541999 ; https://behaviormodel.org/
- Behaviour = Motivation × Ability × Prompt at the same moment; if a behaviour does not happen, one is missing.
  Strength: model/argument with practitioner uptake.

### G2. Wood & Neal 2007 and Wood, Quinn & Kashy 2002 — habits
- Wood & Neal, Psychological Review 114(4):843–863. https://doi.org/10.1037/0033-295X.114.4.843 — habits are
  context-cued responses learned slowly by repetition; once formed, context triggers them without a mediating
  goal; goals shape habits mainly by driving repetition and cue exposure.
- Wood, Quinn & Kashy, JPSP 83(6):1281–1297. https://doi.org/10.1037/0022-3514.83.6.1281 — hourly-diary
  studies; habitual acts were accompanied by unrelated thoughts and **less stress** than non-habitual acts. The
  oft-quoted "~45% of daily behaviour is habitual" comes from these diaries **[not re-verified]**.
- Strength: strong theory and diary evidence.

### G3. GTD and review rituals — Heylighen & Vidal 2008
- Long Range Planning 41(6):585–605. https://doi.org/10.1016/j.lrp.2008.09.004 ;
  PDF http://pespmc1.vub.ac.be/Papers/GTD-cognition.pdf
- Explains GTD via situated/distributed cognition — the brain offloads memory and triggering to the environment;
  the **weekly review** re-establishes an overview of all actionable lists. The authors state that **no empirical
  studies confirm GTD's claims**; justification is theoretical. Bellotti 2003 (A5) supplies field evidence that
  email already serves as the task list. Strength: theory-only for the ritual; moderate for the mechanism.

### G4. Oulasvirta, Rattenbury, Ma & Raita 2012 — "Habits make smartphone use more pervasive"
- Personal and Ubiquitous Computing 16(1):105–114. https://doi.org/10.1007/s00779-011-0412-2
- Logs and field studies: a **checking habit** — brief, repetitive inspection of quickly accessible dynamic
  content — is reinforced by fast informational rewards; it spurs other use and raises total use; experienced as
  annoyance more than addiction. Strength: strong descriptive.

### G5. Notification-driven checking (cross-references)
- B3 (63.5/day, social pressure to respond), A8 (77 checks/day; notification-driven checkers fare worse), B4
  (three batches/day best; none is worse), B5 (three checks/day lowers stress), G4 (habit loop).

---

## 1. Twenty design principles derived from the evidence

1. **Batch, don't stream.** Deliver non-urgent signals in a few predictable batches per day; never remove them
   entirely. — B4, B5, A8, B3. *Strong.*
2. **Make checking a deliberate act, not a reflex.** Prefer user-initiated review over push; if you push, push
   rarely at context-appropriate moments. — A8, B2, G4, D7 G3. *Moderate–strong.*
3. **Every open item costs attention; count decisions, not messages.** The reliable effect is the pull to resume,
   not memory for the unfinished. — C6, A11, G3. *Moderate.*
4. **Keep simultaneous unchunked choices at four or fewer.** — C3, C1, C5. *Strong for the limit; moderate for
   the UI corollary.*
5. **Disclose progressively, two levels deep at most.** — C5, C4. *Moderate.*
6. **Assume the user is the stronger judge on the borderline, and design the split of labour explicitly.**
   Human+AI teams lose on decision tasks by default. — D14, D8. *Strong.*
7. **Show instance-level confidence; do not rely on explanations to calibrate trust.** — D10, D8, D11. *Strong.*
8. **Make the AI's involvement cost a small deliberate act when stakes are high** (decide-first, on-demand or
   delayed reveal), and accept a satisfaction penalty. — D9, D11. *Strong.*
9. **Let people edit the algorithm, even slightly.** Modifiability restores use after visible errors. — D6, D5.
   *Strong.*
10. **Explain soundly and completely within a bounded vocabulary, and make every explanation element editable,
    reversible, honoured and incremental.** — D12, D13. *Strong (one lab), directly on-domain.*
11. **Set alert thresholds with the base rate in mind; false alarms produce disuse.** — D2, D4. *Strong.*
12. **Prefer undo to confirmation; reserve confirmations for rare, irreversible, named actions.** — E6, E1, E4,
    E5. *Moderate–strong.*
13. **Close the gulf of evaluation: after every action show what changed; after every automated action show
    why.** — E3, D7 G11/G16, E2 #1. *Strong (heuristic).*
14. **Speak the user's language; no metaphor beats a metaphor that leaks.** — F1, F2, F3, E2 #2. *Strong for
    jargon; moderate for metaphor.*
15. **Onboard in context, on pull, never with a tour.** — F5, F4, C5. *Moderate.*
16. **Show the work while it runs; users value visible effort.** — F6, C11. *Strong.*
17. **Let users author and finish their own rules; ownership raises valuation only on completion.** — F7, D6, D12.
    *Strong.*
18. **Design for a ritual: a cue, a small ability cost, and a visible end.** — G1, G2, G3, C9, C10. *Moderate.*
19. **Respect response-time limits: 0.1 s for direct manipulation, 1 s to keep flow, progress indicator past
    10 s.** — C11. *Strong rule of thumb.*
20. **Do not build on decision fatigue or choice overload as laws; build on interruption cost and bounded working
    memory, which replicate.** — C7, C8, B1, C3. *Contested (former); strong (latter).*

---

## 2. Applied to Riddle

Riddle: desktop Gmail triage; rules DSL + ML signals; views Finds / Unresolved / Inbox / Spoil / Contamination;
Activity ledger with undo; Director LLM proposing rule changes for approval; autonomy ladder Observe < Suggest <
Act < Trusted; archaeology metaphor throughout.

1. **Display confidence per message as a calibrated number or band, and reserve Finds for high-confidence,
   high-stakes items.** Zhang 2020: confidence scores calibrate switching while explanations do not; Bansal 2021:
   explanations raise acceptance regardless of correctness. So a Find card leads with confidence and the *rule
   that fired* (a sound, complete, bounded explanation per Kulesza 1.1–1.3), not a persuasive narrative. Never
   show a confidence the model cannot back: Vodrahalli's inflated-confidence result is a trust trap, and Dietvorst
   2015 shows one visible error costs more than a human's would.

2. **Treat Unresolved as the primary human task, not an error bucket.** Vaccaro 2024: teams win only where the
   human is the stronger judge — exactly the borderline cases. Put Unresolved first in the daily review, cap the
   visible batch at ≤4 decisions (C3), and make each resolution a rule-teaching moment (D12: instance label plus
   optional feature edit). An undecided message is a Kulesza "Alice" case — too few examples — so ask for a
   feature, not just a label.

3. **Design the correction loop as Explanatory Debugging.** For every classification show which rule/signal fired,
   the top ≤10 contributing features with weights, and the prior. Let the user edit any element (add/remove a
   feature, nudge a weight, add a rule), re-classify *immediately* and show the delta on affected messages (2.4
   incremental), keep every edit undoable (2.2), and never silently discard a correction (2.3). Kulesza 2011's top
   information need was "what happens if I change this": a preview count ("this rule would move 37 messages")
   answers it before commit.

4. **Avoid automation bias in Finds with cognitive forcing where it matters.** Buçinca 2021: on-demand,
   decide-first and delayed reveal cut overreliance at a satisfaction cost. Apply selectively: in **Spoil** and
   **Contamination** (the views that can hide mail) show the message summary *before* Riddle's verdict, verdict
   on demand; in Finds (surfacing, low harm) show the verdict up front. The Never Hidden gate is the structural
   form of "scope services when in doubt" (G10).

5. **Use the autonomy ladder as a two-dimensional control, and change rungs by evidence.** Parasuraman–Sheridan–
   Wickens: automate information acquisition and analysis freely (Observe/Suggest), be cautious automating
   decision and action (Act/Trusted). Shneiderman: high automation *and* high control, so Trusted still exposes an
   audit trail (the Activity ledger is the flight recorder) and a one-click global control (G17). Promote a rule
   from Suggest to Act only after N approvals with zero reversals (G14); demote on a user reversal. Lee & See's
   resolution: trust should track *per-rule* capability, so the rung is per rule/route, not global.

6. **Prefer undo to confirmation wherever the ledger can reverse; confirm only irreversible Gmail writes.**
   Nielsen 2018 and Shneiderman rule 6: confirmations habituate; undo lowers anxiety. Archive, label, move → act
   with a toast + Undo (the Gmail Undo Send pattern: delay the irreversible write for a grace period). Permanent
   delete, report-spam, unsubscribe → a specific, named confirmation ("Delete 12 messages from Acme Newsletters")
   with the verb on the button, never "OK".

7. **Batch write-backs and notifications to three moments a day by default.** Fitz 2019 (3×/day best, hourly ≈
   nothing, none → FoMO), Kushlev & Dunn 2015, Mark 2016 (notification-driven checkers do worse). Sync silently
   and surface one "dig report" at user-chosen times; a badge count of *decisions pending* (not messages) is the
   only ambient signal. Offer an "urgent sender" bypass, because Pielot 2014 shows people re-check more when they
   fear missing something.

8. **Make the daily review a bounded ritual with a visible end.** Fogg: cue (scheduled report), ability (≤4
   decisions on screen, keyboard shortcuts), prompt (the badge). Peak–end: close the review with what was resolved
   and learned ("2 rules improved"); endow progress (Nunes & Drèze) by showing the batch as already partly done
   when Riddle auto-resolved part of it. GTD's weekly review has no empirical support but the mechanism
   (re-establishing overview of open loops) is sound; a weekly "site survey" of rule health fits.

9. **The archaeology metaphor is a learnability cost; pay it deliberately or drop it.** Carroll & Thomas: bad
   metaphors slow learning; Gentner & Nielsen: metaphors leak where source and target differ. "Spoil" and
   "Contamination" do not map onto users' existing email vocabulary (Jakob's law: Archive, Spam, Promotions).
   Shulman 2020: jargon hurts even with definitions. If the metaphor stays: pair every term with a plain gloss
   ("Spoil — low-value mail"), keep Gmail's own verbs for actions, and never use metaphor words in confirmations
   or errors (E2 #9). Spool's test: can a Gmail user act correctly on first sight? If not, the term is costing
   you. Norman 2005 licenses *some* learning for a power tool, but only for genuinely new concepts (a "Find" is
   arguably new; "Spoil" is just "junk").

10. **Onboard by pull, not tour.** NN/g 2023: tours are skipped and forgotten. First run should sync in Observe,
    show real messages with real classifications, and attach a dismissable, recallable tip to the first Find, the
    first Unresolved item and the first Director proposal. Use empty states to explain a view when it is empty.
    Labour illusion (Buell & Norton): during first sync show what Riddle is doing (messages scanned, rules
    evaluated) rather than a spinner.

11. **Make Director proposals modifiable, small and previewable.** Dietvorst 2018: even slight modification
    restores use of an imperfect algorithm. Every proposal should be an editable rule diff with a dry-run count of
    affected messages, an "adjust" affordance (not just approve/reject) and a rollback. The IKEA effect says users
    value rules they finished themselves, so let the Director *start* a rule and the user *finish* it. One visibly
    wrong auto-applied proposal costs more trust than many correct ones (D5), which argues for Suggest as the
    default rung.

12. **Set Signal thresholds with base rates and expose the false-alarm rate.** Parasuraman & Riley: cry-wolf
    alarms cause disuse. Show per-rule precision from the ledger ("reversed 3 of 40 times") — Amershi G2 at the
    level of the rule. A rule whose reversal rate climbs is demoted to Suggest automatically and the user told
    (G18).

13. **Keep the ledger the source of truth for trust.** Shneiderman's audit trail, Lee & See's process/performance
    display, Amershi G12/G16: the Activity view should answer "what did Riddle do to my mail this week, why, and
    can I reverse it" on one screen. "Undo All" for a sync run is the strongest anxiety-reducer Riddle has; keep it
    prominent.

14. **Respect response-time limits in the review loop.** Reclassification preview after a rule edit should return
    within ~1 s to keep the correction loop in flow (Nielsen; Doherty's sub-second gain); a full re-sync past 10 s
    needs a percent-done indicator with visible work (F6).

15. **Do not design against decision fatigue; design against interruption.** The hungry-judge and ego-depletion
    effects are contested (C7). What replicates: interruption raises stress even when work gets done (Mark 2008)
    and alerts derail people for 10–25 minutes (Iqbal & Horvitz). Riddle's value is *fewer, calmer, batched*
    touches with Gmail, not "less willpower per decision".

---

## 3. Sources

Primary papers (DOI or author PDF)
- Whittaker & Sidner 1996 — https://doi.org/10.1145/238386.238530
- Fisher, Brush, Gleave & Smith 2006 — https://doi.org/10.1145/1180875.1180922
- Dabbish, Kraut, Fussell & Kiesler 2005 — https://doi.org/10.1145/1054972.1055068 ; https://sfussell.hci.cornell.edu/pubs/Manuscripts/Dabbish_CHI2005.pdf
- Dabbish & Kraut 2006 — https://doi.org/10.1145/1180875.1180941
- Bellotti, Ducheneaut, Howard & Smith 2003 — https://doi.org/10.1145/642611.642672
- González & Mark 2004 — https://doi.org/10.1145/985692.985707 ; https://ics.uci.edu/~gmark/CHI2004.pdf
- Mark, Voida & Cardello 2012 — https://doi.org/10.1145/2207676.2207754 ; https://ics.uci.edu/~gmark/Home_page/Publications_files/CHI%202012.pdf
- Mark, Iqbal, Czerwinski, Johns & Sano 2016 — https://doi.org/10.1145/2858036.2858262 ; https://ics.uci.edu/~gmark/Home_page/Publications_files/CHI%2016%20Email%20Duration.pdf
- Mark, Gudith & Klocke 2008 — https://doi.org/10.1145/1357054.1357072 ; https://ics.uci.edu/~gmark/chi08-mark.pdf
- Iqbal & Horvitz 2007 — https://doi.org/10.1145/1240624.1240730 ; https://www.microsoft.com/en-us/research/wp-content/uploads/2016/11/CHI_2007_Iqbal_Horvitz-1.pdf
- Pielot, Church & de Oliveira 2014 — https://doi.org/10.1145/2628363.2628364 ; https://pielot.org/pubs/Pielot2014-MobileHCI-Notifications-InfoBites.pdf
- Fitz et al. 2019 — https://doi.org/10.1016/j.chb.2019.07.016
- Kushlev & Dunn 2015 — https://doi.org/10.1016/j.chb.2014.11.005 ; https://www.interruptions.net/literature/Kushlev-ComputHumBehav15.pdf
- Aberdeen, Pacovsky & Slater 2010 — https://research.google/pubs/pub36955
- Kalman, Ravid, Raban & Rafaeli 2006 — https://doi.org/10.1111/j.1083-6101.2006.00312.x ; Kalman & Rafaeli 2011 — https://doi.org/10.1177/0093650210378229
- Cecchinato, Cox & Bird 2015 — https://doi.org/10.1145/2702123.2702537
- Hyman 1953 — https://doi.org/10.1037/h0056940 ; Fitts 1954 — https://doi.org/10.1037/h0055392 ; MacKenzie 1992 — https://doi.org/10.1207/s15327051hci0701_3
- Miller 1956 — https://doi.org/10.1037/h0043158 ; Cowan 2001 — https://doi.org/10.1017/S0140525X01003922 ; Sweller 1988 — https://doi.org/10.1207/s15516709cog1202_4
- Ghibellini & Meier 2025 — https://doi.org/10.1057/s41599-025-05000-w
- Danziger et al. 2011 — https://doi.org/10.1073/pnas.1018033108 ; Weinshall-Margel & Shapard 2011 — https://doi.org/10.1073/pnas.1110910108 ; Glöckner 2016 — http://journal.sjdm.org/16/16823/jdm16823.pdf
- Hagger et al. 2016 — https://doi.org/10.1177/1745691616652873
- Iyengar & Lepper 2000 — https://doi.org/10.1037/0022-3514.79.6.995 ; Scheibehenne, Greifeneder & Todd 2010 — https://doi.org/10.1086/651235
- Nunes & Drèze 2006 — https://doi.org/10.1086/500480
- Kahneman et al. 1993 — https://doi.org/10.1111/j.1467-9280.1993.tb00589.x ; Fredrickson & Kahneman 1993 — https://doi.org/10.1037/0022-3514.65.1.45 ; Redelmeier & Kahneman 1996 — https://doi.org/10.1016/0304-3959(96)02994-6 ; Alaybek et al. 2022 — https://doi.org/10.1016/j.obhdp.2022.104149
- Murdock 1962 — https://doi.org/10.1037/h0045106
- Doherty & Thadani 1982 (reproduction) — https://jlelliotton.blogspot.com/p/the-economic-value-of-rapid-response.html
- Lee & See 2004 — https://doi.org/10.1518/hfes.46.1.50_30392 ; Parasuraman & Riley 1997 — https://doi.org/10.1518/001872097778543886 ; Parasuraman, Sheridan & Wickens 2000 — https://doi.org/10.1109/3468.844354
- Mosier, Skitka, Heers & Burdick 1998 — https://doi.org/10.1207/s15327108ijap0801_3 ; Skitka, Mosier & Burdick 1999 — https://doi.org/10.1006/ijhc.1999.0252 ; Goddard, Roudsari & Wyatt 2012 — https://doi.org/10.1136/amiajnl-2011-000089
- Dietvorst, Simmons & Massey 2015 — https://doi.org/10.1037/xge0000033 ; 2018 — https://doi.org/10.1287/mnsc.2016.2643 ; PDF https://faculty.wharton.upenn.edu/wp-content/uploads/2016/08/Dietvorst-Simmons-Massey-2018.pdf
- Amershi et al. 2019 — https://doi.org/10.1145/3290605.3300233 ; https://www.microsoft.com/en-us/research/wp-content/uploads/2019/01/Guidelines-for-Human-AI-Interaction-camera-ready.pdf
- Bansal et al. 2021 — https://doi.org/10.1145/3411764.3445717 ; https://arxiv.org/abs/2006.14779
- Buçinca, Malaya & Gajos 2021 — https://doi.org/10.1145/3449287 ; https://arxiv.org/abs/2102.09692
- Zhang, Liao & Bellamy 2020 — https://doi.org/10.1145/3351095.3372852 ; https://arxiv.org/abs/2001.02114
- Prabhudesai et al. 2023 — https://doi.org/10.1145/3581641.3584033 ; Ma et al. 2023 — https://doi.org/10.1145/3544548.3581058 ; Vodrahalli, Gerstenberg & Zou 2022 — NeurIPS 2022
- Kulesza, Burnett, Wong & Stumpf 2015 — https://doi.org/10.1145/2678025.2701399 ; https://openaccess.city.ac.uk/id/eprint/13819/1/paper326.pdf
- Kulesza et al. 2011 — https://doi.org/10.1145/2030365.2030367 ; https://openaccess.city.ac.uk/id/eprint/12414/10/TiiS-1129-1149.pdf ; Kulesza et al. 2010 — https://doi.org/10.1109/VLHCC.2010.15
- Stumpf et al. 2007 — https://web.engr.oregonstate.edu/~burnett/Reprints/iui07-mlexplanations.pdf ; Stumpf et al. 2009 — https://doi.org/10.1016/j.ijhcs.2009.03.004
- Vaccaro, Almaatouq & Malone 2024 — https://doi.org/10.1038/s41562-024-02024-1 ; https://arxiv.org/abs/2405.06087
- Shneiderman 2020 — https://doi.org/10.1080/10447318.2020.1741118 ; https://arxiv.org/abs/2002.04087
- Norman 2005 — https://doi.org/10.1145/1070960.1070976 ; https://jnd.org/human-centered-design-considered-harmful/
- Abowd & Dix 1992 — https://doi.org/10.1016/0953-5438(92)90021-7 ; https://www.alandix.com/academic/papers/undo92/undo92.html
- Carroll & Thomas 1982 — https://doi.org/10.1109/TSMC.1982.4308795 ; Gentner & Nielsen 1996 — https://doi.org/10.1145/232014.232032
- Oppenheimer 2006 — https://doi.org/10.1002/acp.1178 ; Shulman et al. 2020 — https://doi.org/10.1177/0261927X20902177
- Buell & Norton 2011 — https://doi.org/10.1287/mnsc.1110.1376 ; Norton, Mochon & Ariely 2012 — https://doi.org/10.1016/j.jcps.2011.08.002
- Fogg 2009 — https://doi.org/10.1145/1541948.1541999 ; https://behaviormodel.org/
- Wood & Neal 2007 — https://doi.org/10.1037/0033-295X.114.4.843 ; Wood, Quinn & Kashy 2002 — https://doi.org/10.1037/0022-3514.83.6.1281
- Heylighen & Vidal 2008 — https://doi.org/10.1016/j.lrp.2008.09.004 ; http://pespmc1.vub.ac.be/Papers/GTD-cognition.pdf
- Oulasvirta, Rattenbury, Ma & Raita 2012 — https://doi.org/10.1007/s00779-011-0412-2

Practitioner and secondary sources
- NN/g: Ten usability heuristics — https://www.nngroup.com/articles/ten-usability-heuristics/ ; Progressive disclosure — https://www.nngroup.com/articles/progressive-disclosure/ ; Confirmation dialogs — https://www.nngroup.com/articles/confirmation-dialog/ ; Response time limits — https://www.nngroup.com/articles/response-times-3-important-limits/ ; Onboarding tutorials vs contextual help — https://www.nngroup.com/articles/onboarding-tutorials/ ; Match between system and real world — https://www.nngroup.com/articles/match-system-real-world/ ; Anti-Mac interface — https://www.nngroup.com/articles/anti-mac-interface/
- Shneiderman, Eight Golden Rules — https://www.cs.umd.edu/users/ben/goldenrules.html
- Spool, What makes a design seem intuitive — https://articles.centercentre.com/design_intuitive/
- Laws of UX — https://lawsofux.com/ (hicks-law, fittss-law, millers-law, teslers-law, jakobs-law, von-restorff-effect, serial-position-effect, zeigarnik-effect, doherty-threshold, peak-end-rule, goal-gradient-effect)
- Merlin Mann / Inbox Zero — https://en.wikipedia.org/wiki/Merlin_Mann ; https://www.youtube.com/watch?v=z9UjeTMb3Yk
- Gmail Undo Send — https://gmail.googleblog.com/2009/03/new-in-labs-undo-send.html ; https://time.com/3932586/google-gmail-undo-send/
- Newport, A World Without Email — https://www.calnewport.com/books/a-world-without-email/
- Norman, The Design of Everyday Things — https://en.wikipedia.org/wiki/The_Design_of_Everyday_Things

## Verification notes
- Read in full (author PDFs): Mark 2012, Mark 2016, Mark 2008, González & Mark 2004, Iqbal & Horvitz 2007, Kushlev
  & Dunn 2015, Fitz 2019, Aberdeen 2010, Dabbish 2005, Kulesza 2015, Kulesza 2011, Stumpf 2007, Dietvorst 2015 and
  2018, Buçinca 2021, Zhang 2020, Bansal 2021, Vaccaro 2024, Shneiderman 2020, Glöckner 2016, Iyengar & Lepper
  2000, Amershi 2019, Heylighen & Vidal 2008.
- Verified by abstract/record only (ACM, OpenAlex, Semantic Scholar, publisher pages): Whittaker & Sidner 1996,
  Fisher 2006, Dabbish & Kraut 2006, Bellotti 2003, Cecchinato 2015, Pielot 2014, Lee & See 2004, Parasuraman &
  Riley 1997, PSW 2000, Mosier 1998, Skitka 1999, Danziger 2011, Hagger 2016, Scheibehenne 2010, Nunes & Drèze
  2006, Redelmeier & Kahneman 1996, Norton et al. 2012, Buell & Norton 2011, Wood & Neal 2007, Wood et al. 2002,
  Oulasvirta 2012, Fogg 2009, Oppenheimer 2006, Shulman 2020, Carroll & Thomas 1982, Ghibellini & Meier 2025,
  Kalman 2006/2011, Prabhudesai 2023, Ma 2023, Vodrahalli 2022, Goddard 2012, Cowan 2001, Murdock 1962.
- Not retrieved: Alaybek 2022 abstract; Whittaker & Sidner 1996 body statistics; Lee & See's calibration/
  resolution/specificity passage; the "45% habitual" figure. Each is flagged inline. The popular "23 minutes to
  recover" number is not in Mark, Gudith & Klocke 2008 and should not be cited to it.
