# Family Relation Extraction — 5% sample

**Qualifying chunks (≥2 entities + trigger):** 211  
**Sampled:** 11  
**Model:** llama3.1:70b-instruct-q3_K_M  
**Commit:** dry-run

---

## Chunk 1 / 11  (id=-9102990948384786174)

**Section:** Chapter Nineteen  All Africa Convention  
**Doc:** LEST WE FORGET -rev25.pdf  chunk #989

**Entities in chunk:**
  - Goolam Gool  (also: Goulam Gool, Dr Goolam Gool, Dr. Goolam, Goolam (GH))
  - Haji Joosub Maulvi Hamid Gooli  (also: Joosub Maulvi Hamid Gooli, Haji Joosub Maulvi Hamid Gool, J.M.H. Gool, JMH Gool)
  - Hamid  (also: Maulvi Hamid)
  - Hamid Khan
  - Hanief Ally  (also: Uncle Hanief, Sayed Hanief Ally, Hanief)
  - Hans Friederichs  (also: Hans)
  - Hansie Kroneberg
  - Harold Cressy
  - Harold Kruger
  - Harold Wolpe
  - Harry Lawrence
  - Hartog
  - Hassan Bavasah
  - Hassen Abrahams
  - Helen Abrahams
  - Herbert
  - Jane Gool-Tabata  (also: Jane Gool, Janab)
  - Zobeida Gool  (also: Billy, Zobeida (Billy), Zobeida Billy, Zobeida)

**Triggers found:**  uncle 

**CC pass (raw):**
```json
{"quote": "my uncle Goolam Gool"}
```

**CC quote:** `my uncle Goolam Gool`  ✅ anchored → EC

**EC pass (raw):**
```json
[non-schema relation — EC skipped]
```

**Extracted relations:** none

---

## Chunk 2 / 11  (id=-9073283356142808755)

**Doc:** LEST WE FORGET -rev25.pdf  chunk #477

**Entities in chunk:**
  - Bartnicke
  - Davids  (also: Mr Davids)
  - Geoff
  - Mr. Slingers  (also: Slingers)
  - Nicky Springveld  (also: Springveld)
  - Rashid Zaheer Nurjahan
  - Ronnie Jooste  (also: Ronnie)
  - Shawquet
  - Tiddles
  - Yousuf Rassool  (also: Yusuf Rassool, Joe Rassool, Yousuf (Joe) Rassool, Y.S. Rassool)
  - Yusuf

**Triggers found:** mother,  cousin

**CC pass (raw):**
```json
{"quote": "none"}
```

**CC quote:** none — EC pass skipped

**Extracted relations:** none

---

## Chunk 3 / 11  (id=-9022111028918856016)

**Doc:** LEST WE FORGET -rev25.pdf  chunk #790

**Entities in chunk:**
  - Ben Malamba
  - Dora Taylor
  - James Africa
  - Nosipho Majeke
  - Rubaiyat Politically
  - Salie Van Haacht
  - Stanley Abrahams
  - Trade Unionists
  - Woody Woodman
  - Yousuf Rassool  (also: Yusuf Rassool, Joe Rassool, Yousuf (Joe) Rassool, Y.S. Rassool)

**Triggers found:** wed , sister

**CC pass (raw):**
```json
{"quote": "none"}
```

**CC quote:** none — EC pass skipped

**Extracted relations:** none

---

## Chunk 4 / 11  (id=-8746970108171972112)

**Section:** Chapter Twenty    Split in the Movement  
**Doc:** LEST WE FORGET -rev25.pdf  chunk #1018

**Entities in chunk:**
  - Ayesha Rassool  (also: Lallie, Ayesha, Lallie Gool, Ayesha Gool)
  - Cissie Gool  (also: Zainunnisa Cissie Gool, Zainunnisa Gool, Zainunnissa Cissie Gool, Zainunnissa Gool)
  - Dija
  - Fazil Rassool  (also: Fazil)
  - Haji Joosub Maulvi Hamid Gooli  (also: Joosub Maulvi Hamid Gooli, Haji Joosub Maulvi Hamid Gool, J.M.H. Gool, JMH Gool)
  - Morris
  - Toetie Ally  (also: Toetie, Auntie Toetie)
  - Zohra Abdurahman  (also: Zohra (Auntie Jolly), Auntie Jolly, Aunt Jolly, Jolly (Zohra))

**Triggers found:** mother, brother

**CC pass (raw):**
```json
{"quote": "brother Fazil"}
```

**CC quote:** `brother Fazil`  ✅ anchored → EC

**EC pass (raw):**
```json
{"relations":[{"from":"Fazil Rassool","relation":"sibling_of","to":"Ayesha Rassool"}]}
```

**Extracted relations:**
- `Fazil Rassool` **sibling_of** `Ayesha Rassool`

---

## Chunk 5 / 11  (id=-8694007978331675509)

**Doc:** LEST WE FORGET -rev25.pdf  chunk #127

**Entities in chunk:**
  - Abdul Kadar
  - Bibi Gool  (also: Bibi, Peari Beghum)
  - Haji Joosub Maulvi Hamid Gooli  (also: Joosub Maulvi Hamid Gooli, Haji Joosub Maulvi Hamid Gool, J.M.H. Gool, JMH Gool)
  - Lachiram
  - Mohandas Karamchand Gandhi  (also: MK Gandhi, M.K. Gandhi, M_ Gandhi, M. Gandhi)
  - Mr. Yusuf Hamid Gool  (also: Yusuf Hamid Gool)
  - Victor

**Triggers found:** father

**CC pass (raw):**
```json
{"quote": "none"}
```

**CC quote:** none — EC pass skipped

**Extracted relations:** none

---

## Chunk 6 / 11  (id=-8621339613752899196)

**Doc:** LEST WE FORGET -rev25.pdf  chunk #943

**Entities in chunk:**
  - Abbas Dinath
  - Dr. Patel  (also: Patel)
  - E.S Reddy Former  (also: E.S. Reddy Former)
  - Goolam Gool  (also: Goulam Gool, Dr Goolam Gool, Dr. Goolam, Goolam (GH))
  - Haji Joosub Maulvi Hamid Gooli  (also: Joosub Maulvi Hamid Gooli, Haji Joosub Maulvi Hamid Gool, J.M.H. Gool, JMH Gool)
  - MQotsi
  - Mohandas Karamchand Gandhi  (also: MK Gandhi, M.K. Gandhi, M_ Gandhi, M. Gandhi)
  - Mr. Messaris  (also: Messaris)
  - Ralph J Bunche  (also: Ralph Bunche)

**Triggers found:** son , sister

**CC pass (raw):**
```json
{"quote": "one of Gandhi’s sons fell in love with one of Goulam Gool’s sisters"}
```

**CC quote:** `one of Gandhi’s sons fell in love with one of Goulam Gool’s sisters`  ✅ anchored → EC

**EC pass (raw):**
```json
{"relations":[{"from":"Mohandas Karamchand Gandhi","relation":"parent_of","to":"Goolam Gool’s sister’s spouse"}]}
```

**Extracted relations:** none

---

## Chunk 7 / 11  (id=-8512653961804288406)

**Doc:** LEST WE FORGET -rev25.pdf  chunk #872

**Entities in chunk:**
  - Alie Fataar
  - Balthazzar John Vorster
  - Cosmo Pieterse
  - E.L.Maurice
  - F.Landman
  - Geoffrey Abrahams  (also: Geoff Abrahams)
  - Irwin Combrinck  (also: Irwin Combrick, Irwin)
  - Joyce Meissenheimer  (also: Joyce Meisenheimer)
  - Victor Wessels

**Triggers found:** son ,  aunt 

**CC pass (raw):**
```json
{"quote": "none"}
```

**CC quote:** none — EC pass skipped

**Extracted relations:** none

---

## Chunk 8 / 11  (id=-8435946538044972915)

**Doc:** LEST WE FORGET -rev25.pdf  chunk #87

**Entities in chunk:**
  - Bibi Gool  (also: Bibi, Peari Beghum)
  - Haji Joosub Maulvi Hamid Gooli  (also: Joosub Maulvi Hamid Gooli, Haji Joosub Maulvi Hamid Gool, J.M.H. Gool, JMH Gool)
  - Lord Roberts

**Triggers found:** wife, husband, married, father, mother, sister, niece, nephew

**CC pass (raw):**
```json
{"quote": "my grandfather departed from Rander leaving behind his mother, sisters and wife all weeping their hearts out."}
```

**CC quote:** `my grandfather departed from Rander leaving behind his mother, sisters and wife all weeping their hearts out.`  ✅ anchored → EC

**EC pass (raw):**
```json
Since the narrator is 'my', we can infer that the narrator's name is Bibi Gool (also known as Bibi, Peari Beghum). The quote states that "my grandfather departed", so we can establish a relationship between the narrator and their grandfather.

However, there is no mention of Haji Joosub Maulvi Hamid Gooli or Lord Roberts in this context. Therefore, based on the given rules, we cannot extract any relationships involving these individuals.

The quote does mention the grandfather's mother, sisters, and wife, but it does not provide their names as canonical names from the list. Therefore, we cannot establish any relationships with them either.

Since there is no valid relationship to extract, the correct response would be:

{"relations":[]}
```

**Extracted relations:** none

---

## Chunk 9 / 11  (id=-8167953885918041797)

**Doc:** LEST WE FORGET -rev25.pdf  chunk #396

**Entities in chunk:**
  - Abba (Louis Messaris)  (also: Abba)
  - Churchill Smuts  (also: Churchill)
  - Cissie Gool  (also: Zainunnisa Cissie Gool, Zainunnisa Gool, Zainunnissa Cissie Gool, Zainunnissa Gool)
  - Field Marshal Smuts
  - George
  - Haji Joosub Maulvi Hamid Gooli  (also: Joosub Maulvi Hamid Gooli, Haji Joosub Maulvi Hamid Gool, J.M.H. Gool, JMH Gool)
  - Harry Snitcher
  - Moses Kotane
  - Musto
  - Ray Alexander  (also: Alexander)
  - Smuts

**Triggers found:** wed ,  uncle 

**CC pass (raw):**
```json
{"quote": "none"}
```

**CC quote:** none — EC pass skipped

**Extracted relations:** none

---

## Chunk 10 / 11  (id=-8054118982771437578)

**Doc:** LEST WE FORGET -rev25.pdf  chunk #768

**Entities in chunk:**
  - Abba (Louis Messaris)  (also: Abba)
  - Haji Joosub Maulvi Hamid Gooli  (also: Joosub Maulvi Hamid Gooli, Haji Joosub Maulvi Hamid Gool, J.M.H. Gool, JMH Gool)
  - Hymie Beimel  (also: Hymie)
  - Jane Gool-Tabata  (also: Jane Gool, Janab)
  - Phillis Ntantala Jordan
  - Victor
  - Willem Van Schoor  (also: W.P. Van Schoor, W.P Van Schoor)
  - Yousuf Rassool  (also: Yusuf Rassool, Joe Rassool, Yousuf (Joe) Rassool, Y.S. Rassool)

**Triggers found:** wed ,  aunt 

**CC pass (raw):**
```json
{"quote": "my aunt Jane Gool"}
```

**CC quote:** `my aunt Jane Gool`  ✅ anchored → EC

**EC pass (raw):**
```json
[non-schema relation — EC skipped]
```

**Extracted relations:** none

---

## Chunk 11 / 11  (id=-8052727503191672567)

**Doc:** LEST WE FORGET -rev25.pdf  chunk #23

**Entities in chunk:**
  - Bibi Gool  (also: Bibi, Peari Beghum)
  - Field Marshal Tito
  - General Smuts
  - Haji Joosub Maulvi Hamid Gooli  (also: Joosub Maulvi Hamid Gooli, Haji Joosub Maulvi Hamid Gool, J.M.H. Gool, JMH Gool)
  - Hassen Mall  (also: Hassen)
  - Rebecca Rassool
  - Yousuf Rassool  (also: Yusuf Rassool, Joe Rassool, Yousuf (Joe) Rassool, Y.S. Rassool)

**Triggers found:** father

**CC pass (raw):**
```json
{"quote": "My father, Yousuf Rassool"}
```

**CC quote:** `My father, Yousuf Rassool`  ✅ anchored → EC

**EC pass (raw):**
```json
Here is the extracted family relationship in the requested format:

{"relations":[{"from":"Rebecca Rassool","relation":"child_of","to":"Yousuf Rassool"}]}
```

**Extracted relations:**
- `Rebecca Rassool` **child_of** `Yousuf Rassool`

---


## Summary

| Metric | Value |
|--------|-------|
| Chunks processed | 11 |
| Relations extracted | 2 |
| Relations written to graph | 0 |
