# RAG Eval Report

**KB:** `D6`  **Model:** `llama3.1:8b`

**Flags:** top_k=20  hyde=false  rerank=false  understand=false  llm_judge=false

## Summary

| Metric | Value |
|--------|-------|
| Questions | 40 |
| Overall recall (token-overlap) | 67.6% (152/225) |
| Avg latency | 28326ms |

## Per-question results

| ID | Question | Hit rate | Sources | Latency |
|----|----------|----------|---------|--------|
| q01 | Who is the author? | 3/3 (100%) | LEST WE FORGET -rev25.pdf, [Graph: Yousuf Rassool] | 69939ms |
| q02 | Who are the author's children? | 3/3 (100%) | [Graph: Yousuf Rassool], LEST WE FORGET -rev25.pdf | 33846ms |
| q03 | Who are the author's grandchildren? | 6/6 (100%) | LEST WE FORGET -rev25.pdf, [Graph: Yousuf Rassool] | 30963ms |
| q04 | To whom is the book dedicated? | 4/4 (100%) | LEST WE FORGET -rev25.pdf | 26948ms |
| q05 | Who was J.M.H. Gool? | 6/8 (75%) | [Graph: Haji Joosub Maulvi Hamid Gool], LEST WE FORGET -rev25.pdf | 36839ms |
| q06 | Tell me about Buitencingle. | 2/8 (25%) | LEST WE FORGET -rev25.pdf, [Graph: 7 Buitencingle Street] | 30989ms |
| q07 | Who is the author's wife? | 2/3 (67%) | [Graph: Yousuf Rassool], LEST WE FORGET -rev25.pdf | 25342ms |
| q08 | Tell me more about the author's wife. | 2/6 (33%) | [Graph: Yousuf Rassool] | 5332ms |
| q09 | Who was the author's grandfather? | 3/9 (33%) | LEST WE FORGET -rev25.pdf, [Graph: Yousuf Rassool] | 21593ms |
| q10 | Tell me about Kloof Nek. | 6/7 (86%) | LEST WE FORGET -rev25.pdf | 31350ms |
| q11 | What was the Teachers League of South Africa (TLSA)? | 3/6 (50%) | LEST WE FORGET -rev25.pdf, [Graph: Teachers League of South Africa] | 24424ms |
| q12 | Who was Cissie Gool? | 5/6 (83%) | LEST WE FORGET -rev25.pdf, [Graph: Cissie Gool] | 25218ms |
| q13 | What was the All Africa Convention? | 6/6 (100%) | [Graph: All African Convention], LEST WE FORGET -rev25.pdf | 25647ms |
| q14 | Where was District Six and what kind of place was it? | 2/6 (33%) | LEST WE FORGET -rev25.pdf, [Graph: District Six] | 22881ms |
| q15 | What were the forced removals from District Six? | 5/6 (83%) | LEST WE FORGET -rev25.pdf, [Graph: District Six] | 22818ms |
| q16 | Who was Gandhi and what was his connection to the Gool family? | 4/7 (57%) | [Graph: Cissie Gool], LEST WE FORGET -rev25.pdf | 48752ms |
| q17 | What was Hewat Training College? | 4/5 (80%) | LEST WE FORGET -rev25.pdf | 40789ms |
| q18 | What was the New Era Fellowship? | 5/6 (83%) | [Graph: New Era Fellowship], LEST WE FORGET -rev25.pdf | 41114ms |
| q19 | What was the Non-European Unity Movement? | 4/6 (67%) | LEST WE FORGET -rev25.pdf, [Graph: Non-European Unity Movement] | 38715ms |
| q20 | Describe the author's involvement in cricket. | 3/5 (60%) | [Graph: Yousuf Rassool], LEST WE FORGET -rev25.pdf | 38612ms |
| q21 | Who was the author's mother? | 3/5 (60%) | LEST WE FORGET -rev25.pdf, [Graph: Yousuf Rassool] | 26400ms |
| q22 | Who was the author's father? | 4/4 (100%) | [Graph: Yousuf Rassool], LEST WE FORGET -rev25.pdf | 22014ms |
| q23 | Who were the author's siblings? | 5/5 (100%) | LEST WE FORGET -rev25.pdf, [Graph: Yousuf Rassool] | 24392ms |
| q24 | Who were the children of J.M.H. Gool? | 4/7 (57%) | [Graph: Haji Joosub Maulvi Hamid Gool] | 13991ms |
| q25 | Who was I.B. Tabata? | 3/5 (60%) | LEST WE FORGET -rev25.pdf | 24256ms |
| q26 | Who was Dr. Abdullah Abdurahman? | 2/6 (33%) | [Graph: Dr. Abdulla Abdurahman], LEST WE FORGET -rev25.pdf | 20605ms |
| q27 | What was the connection between Gandhi and J.M.H. Gool? | 5/5 (100%) | [Graph: Haji Joosub Maulvi Hamid Gool], LEST WE FORGET -rev25.pdf | 35053ms |
| q28 | Which organisations was the author involved in? | 5/5 (100%) | [Graph: Yousuf Rassool], LEST WE FORGET -rev25.pdf | 25836ms |
| q29 | What was the relationship between the TLSA and the Non-European Unity Movement? | 3/6 (50%) | [Graph: Non-European Unity Movement], LEST WE FORGET -rev25.pdf | 28140ms |
| q30 | When did J.M.H. Gool arrive in Cape Town and from where? | 4/6 (67%) | LEST WE FORGET -rev25.pdf, [Graph: Haji Joosub Maulvi Hamid Gool] | 23553ms |
| q31 | What was the Hanaffi Quwatul Islam Mosque? | 3/6 (50%) | [Graph: Hanaffi Quwatul Islam Mosque], LEST WE FORGET -rev25.pdf | 27784ms |
| q32 | How was Cissie Gool related to J.M.H. Gool? | 5/5 (100%) | LEST WE FORGET -rev25.pdf, [Graph: Cissie Gool] | 29750ms |
| q33 | Who were the notable historical figures that J.M.H. Gool knew personally? | 3/5 (60%) | [Graph: Haji Joosub Maulvi Hamid Gool], LEST WE FORGET -rev25.pdf | 23891ms |
| q34 | What was the Group Areas Act and how did it affect District Six? | 5/6 (83%) | LEST WE FORGET -rev25.pdf, [Graph: District Six] | 26935ms |
| q35 | Who was Hassen Mall? | 4/4 (100%) | LEST WE FORGET -rev25.pdf | 24559ms |
| q36 | What political organisations were active in the Cape Coloured community during the author's lifetime? | 2/6 (33%) | [Graph: Teachers League of South Africa], LEST WE FORGET -rev25.pdf | 29636ms |
| q37 | Who was Mahatma Gandhi and why was he in South Africa? | 4/7 (57%) | [Graph: Teachers League of South Africa], LEST WE FORGET -rev25.pdf | 23149ms |
| q38 | Who was Cissie Gool's father? | 4/5 (80%) | [Graph: Cissie Gool] | 4995ms |
| q39 | What was District Six like as a neighbourhood before the forced removals? | 3/6 (50%) | [Graph: District Six], LEST WE FORGET -rev25.pdf | 27666ms |
| q40 | What was the Unity Movement's boycott policy? | 3/5 (60%) | LEST WE FORGET -rev25.pdf, [Graph: Non-European Unity Movement] | 28355ms |

## Answers

### q01 — Who is the author?

The author is Yousuf (Joe) Rassool [1].

### q02 — Who are the author's children?

The provided sources state that Yousuf Rassool (the author) has three children: 

Feyruz [1]
Reza [1]
Zarina [1]

### q03 — Who are the author's grandchildren?

According to excerpt [13], the author, Yousuf Rassool (Joe Rassool), has children Feyruz, Reza, and Zarina and also grandchildren named Joseph, Samuel, Alexander, Leonardo, Dylan, and Lewis.

### q04 — To whom is the book dedicated?

The book "LEST WE FORGET" is dedicated to the author's children and grandchildren. Specifically, it says: "For my children: Feyruz, Reza, and Zarina and grandchildren, Joseph, Samuel, Alexander, Leonardo, Dylan and Lewis." [18]. 

Note that according to [1], the title of a book read by Joe Rassool is "The Rubaiyat of Omar Khayyam" which mentions a dedication but it's not clear who/what the book is dedicated to.

### q05 — Who was J.M.H. Gool?

[1], [18] indicate that Joosub Maulvi Hamid Gool, abbreviated as J.M.H. Gool, was the author's maternal grandfather and a well-respected individual in Cape Town society at the time. He was an "Oriental trader," a title given to him in Supreme Court transcripts ([17]). According to [13], he had a father who was a Pushto-speaking Maulvi from Swat Province, who moved with his family to Rander in Gujarat, India.

As per information provided in [11], he knew Mahatma Gandhi personally and upon Gokhale's visit to South Africa in 1912, it was Joosub Maulvi Hamid Gool who hosted him at Noor Bagh. Additionally, [18] mentions his correspondence with Gandhi while Abdul Hamid Gool (Joosub's son) was a medical student at Guy’s Hospital in London.

This individual is considered to be a key figure due to his relationships and influence within the community and among influential individuals of that era ([10], [17]).

### q06 — Tell me about Buitencingle.

Buitencingle is mentioned in extracts [1], [6], and [8]. According to these sources:

* Buitencingle was the family home of Haji Joosub Maulvi Hamid Gool, a prosperous merchant.
* It was located on 7 Buitencingle Street, Cape Town (extract [2]).
* The area around Buitencingle was initially well-to-do and cosmopolitan (extract [3]). However, it declined due to the influx of Black and Coloured families, followed by gangs, which drove out many of its white residents.
* Extracts [1] and [6] do not provide substantial information about the layout or structure of Buitencingle. 
* According to [8], Buitencingle was pronounced as "Baytencingle" at one point in history, possibly due to a change in local customs, but its pronunciation wasn't a priority for this question.

The provided sources do not contain a detailed description of Buitencingle beyond these references.

### q07 — Who is the author's wife?

The author's wife is Nazima Rassool [1-13, and again in excerpt [15]].

### q08 — Tell me more about the author's wife.

The provided sources do not contain information about the author's wife beyond her name: professor Nazima Rassool.

### q09 — Who was the author's grandfather?

The author's grandfather was Joosub Maulvi Hamid Gool [18].

### q10 — Tell me about Kloof Nek.

According to the sources, particularly [4], [7], and [17]-[20], Kloof Nek is a connecting saddle of land that joins Table Mountain to Lion's Head in Cape Town. It was an area frequented by lovers, mountaineers, nature lovers, ramblers, and down-and-outs known as "Bergies" who would drink cheap wine and methylated spirits in the cavelets and bushes there.

Kloof Nek Road is also mentioned, specifically that a bus terminus for the single-decker trackless tram from Adderley Street was located there. The area seems to have been easily accessible due to its proximity to Buitencingle, an outer belt of Cape Town.

Several descriptions paint a picture of Kloof Nek as a place of "haunting beauty" [4] and "natural beauty" [20], particularly in the context of visiting or hiking there. [17] mentions a particular experience where the speaker was initially reluctant to visit but then found the area enchanting.

### q11 — What was the Teachers League of South Africa (TLSA)?

The Teachers' League of South Africa, TLSA as mentioned in numerous excerpts [1], [2], and [12] - [15]. From these sources, we understand that it was a non-European teachers' professional body with political inclinations, active against apartheid education policies. Its members were involved in politics, including advocacy and public service, which made authorities take issues seriously.

### q12 — Who was Cissie Gool?

Cissie Gool was a renowned Cape Town politician and community activist. She was the daughter of Dr. Abdullah Abdurahman and served as a Cape Town city councillor. She was known for her fiery speeches and dedication to non-European rights [1]. 

Cissie Gool was married to Abdul Hamid Gool (B.G.). She called a massive demonstration of the Liberation League in 1937, rallying against the government's plan to introduce residential segregation [7]. Her daughter Nasima participated in this meeting at the age of eleven. 

Cissie Gool was also a member of the Non-European Unity Movement and one of the foremost women activists of her time in South Africa [1].

### q13 — What was the All Africa Convention?

The All African Convention (AAC) was a South African political body formed in 1935 and affiliated with the Non-European Unity Movement [1]. It represented African political organisations opposed to the Hertzog Bills (referring to the legislation promoted by General J.B.M. Hertzog, the Prime Minister of the Union of South Africa at that time, which aimed to establish separate development for different racial groups) and continued to oppose apartheid alongside the NEUM [1].

### q14 — Where was District Six and what kind of place was it?

District Six was a vibrant, multi-cultural inner-city neighborhood of Cape Town, South Africa (Source: [1]). It was home to various communities, including the Rassool and Gool families. According to one of the extracts ([4]), one could walk down its streets and see tap dancers in natty straw boaters performing for pennies.

District Six was characterized as having more life, zest, fire, warmth, and care compared to other places (Source: [6]). It was an area where people from different backgrounds lived together, with a rich history of politics and resistance against the apartheid system.

### q15 — What were the forced removals from District Six?

The provided sources describe that in the 1970s and 1980s, the apartheid government forcibly removed the population of District Six under the Group Areas Act. However, there is no specific information available on the number, date ranges, or details of the actual forced removals. 

In [1], a brief overview of the history of District Six states that it was "forcibly removed" and its homes demolished in the 1970s and 1980s.

Source [15] mentions the Group Areas Act (1950) that dispersed District Six's people to the Cape Flats, decimating the community.

### q16 — Who was Gandhi and what was his connection to the Gool family?

Mahatma Gandhi, a renowned Indian independence activist and leader. [2] He was known for being a guest at the Noor Bagh residence of Adam Gool in 1912. There is some indication that Adam Gool married an English nurse, Laura Heffer, and they left the Cape for Australia.

Gandhi also had connections with other members of the Gool family: [13] The correspondence between him and Dr. Abdul Hamid Gool, his uncle was while he was a medical student at Guy’s Hospital in London; [9] Gandhi stayed at the Gool mansion on Buitencingle Street in 1914. According to Indian Opinion (March 1914), Mr. Gokhale visited Noor Bagh in 1912 for a function prior to his departure from Cape Town.

Gandhi was closely associated with the Gool family: In February 1914, Elizabeth Molteno and other European women visited Gandhi at the Gool residence; Gandhi stayed at Groot Schuur, where he was received by Prime Minister Botha and his wife, as well as Mrs. Gladstone.

### q17 — What was Hewat Training College?

According to [5], "Hewat Training College" stood opposite the Roeland Street Gaol in fairly spacious but undeveloped grounds. It had once been a Whites-only Primary school, but the authorities considered it too close to the prison and, more to the point, too close to District Six.

Additionally, from various excerpts, we know that Hewat Training College was established to train teachers for the upper years of the primary division ([8]). The author attended Hewat Training College in 1947 to be trained as a schoolteacher.

### q18 — What was the New Era Fellowship?

The New Era Fellowship (NEF) was an educational and cultural organisation in Cape Town that held public lectures and discussions on political and social issues. It was closely associated with the Non-European Unity Movement, a major anti-apartheid movement in South Africa [1].

The NEF played a significant role in promoting unity among different organisations representing non-Europeans (including Coloured and African communities) to counter the predicted assault on their remaining rights by the government [10].

### q19 — What was the Non-European Unity Movement?

The Non-European Unity Movement (NEUM) was a South African political movement founded in 1943 that united non-European political organisations opposed to apartheid and racial discrimination. It was closely connected to the All African Convention and the Teachers League of South Africa [1]. The NEUM advocated for non-collaboration with apartheid institutions and aimed to create a program-based struggle for equality, rejecting old tactics of deputations and appeals for justice at round table conferences [8].

The movement became influential during that period, as seen in [2] where the author reflects on his interactions with some of its members, particularly Uncle Aity, who was highly respected due to his intellectual background from Aligarh College in India. The NEUM continued to grow and gain traction, becoming a driving force in the struggle against apartheid.

According to [16], the movement decided to have a united front on a local issue regarding train apartheid resistance committees but eventually disintegrated due to disagreements with liberals joining their cause.

In other passages:
[5] mentions how Appolis Slingers was part of an educational conference, detailing changes in the education system for Non-Whites.
[8] provides more information about the founding and history of NEUM, mentioning attempts by DR. Jabavu to create a unity movement in the past.

Overall, the Non-European Unity Movement played a significant role in shaping the struggle against apartheid through its emphasis on non-collaboration and the empowerment of the suppressed communities in South Africa.

### q20 — Describe the author's involvement in cricket.

The author's [1] involvement in cricket is described as follows: He was a keen cricketer who played in the non-European cricket structure in Cape Town. In excerpt [13], he mentions that through his friendship with Hassen Mall, he was learning to play cricket seriously. However, he still considered himself an "absolute rabbit" or a novice at this stage.

In excerpt [14], it is stated that now that the union had moved away from ethnic discrimination, it attracted several non-Indian teams, and what's more, Mr. Hoosain Parker, the de facto Habibia School Manager, brought in a new team: The Orient Cricket Club. 

In the same excerpt, he mentions playing cricket with his forward defensive stroke against Hewat. When Edross saw his defensive stroke, he assured him that had he been a selector for the Indian cricket Union, he would certainly have been his first choice [16].

He also mentions in excerpt [17] that when he began his cricketing days, Kalam was at his peak and our scorebooks had several eighties against his name. 

In excerpt [18], it is stated that Edross saw him play and commented on his batting style, describing him as "Aggressive as a batsman, but gentle as a person with a great sense of humour".

The author also mentions cricket in other excerpts, such as the excitement around the Springbok cricket team's tour to England [16], the biennial panic regarding financing the cricket tour [14], and the union changing the name from "Indian" to "non-European" to include other teams [14].

### q21 — Who was the author's mother?

The provided sources do not contain explicit information about the author's mother. They mention various family members and relatives of the author, but do not specifically describe the author's own mother. 

[1] lists Yousuf’s “Known relationships,” which include his parents (Peter Alexander Rassool, Ayesha Rassool), but does not elaborate on them.

If there is some missing excerpt that provides this information or it might be assumed based on [21], the question cannot be answered accurately given available data.

### q22 — Who was the author's father?

The author's father was Peter Alexander Rassool (also known as Peerbhai or Peru [9]).

### q23 — Who were the author's siblings?

The provided sources do not list all the author's siblings, but [9] mentions the children of Yousuf Rassool as: Feyruz, Reza, and Zarina. Additionally, [1] lists the siblings of Yousuf Rassool as: Abdul, Berina, Fazil, Nasim, Rasheda, Rehana Mohamed (via note), Yasmin, Zain, and a Hana Mohamed is also mentioned in [3].

### q24 — Who were the children of J.M.H. Gool?

According to the source [1], the children of J.M.H. Gool (also referred to as Haji Joosub Maulvi Hamid Gool) are:

1. Abdul Hamid Gool
2. Jane Gool-Tabata
3. Minnie Gool
4. Zobeida Gool
5. Goolam Gool
6. Fatima Gool
7. Mariam Gool
8. Mohamed Saaid Gool

Note: The source also mentions other family relationships, such as the children of the grandchildren, but these are the explicitly stated children listed in the source.

### q25 — Who was I.B. Tabata?

The sources do not explicitly state who I.B. Tabata was, but based on the context, it appears that he was an important figure in the Non-European Unity Movement (NEUM) and a prominent political activist of the time. He is mentioned several times throughout the excerpts, particularly with regard to his book "The Awakening of a People" and its significance within the NEUM.

Source [1] provides some context: "It was an occasion of considerable excitement as it was to be an NEF publication." The fact that the book's publication by the New Era Fellowship (NEF) is significant suggests that I.B. Tabata held a position of leadership or authority in the organization.

Further sources suggest that he played a key role in shaping the NEUM's ideological and strategic direction, particularly with regards to issues of land redistribution and class analysis. However, without more specific information, it is difficult to say much about his personal background or contributions beyond his involvement with the NEUM and its publications.

### q26 — Who was Dr. Abdullah Abdurahman?

The provided sources do not contain comprehensive information about Dr. Abdullah Abdurahman's family or personal background beyond the fact that he was a medical doctor and associated with the Non-European Unity Movement [1].

### q27 — What was the connection between Gandhi and J.M.H. Gool?

According to excerpt [12], Gandhi visited the Gool residence at 7 Buitencingle Street in Cape Town, where he stayed from October 1912. Excerpt [13] states that Dr. Gool offered a farewell address to Gandhi on behalf of Port Elizabeth Indians when Gandhi was leaving South Africa for India in August 1914. Excerpt [14] mentions the close friendship between Gandhi and J.M.H. Gool, which contributed to the latter's connection with Gandhi's ideas.

Gandhi had referred to Dr. Gool as 'Mahatma' before such a time of his gaining this recognition in South Africa (excerpts 5 and 7).

A series of letters, including excerpts [9] and [20], also suggest that there was close interaction between the two families; excerpt [12] mentions Gandhi being addressed personally by Dr. Gool. 

It was in February 1914 when a subscription to the Indian Opinion was sent on behalf of Mr. Wilson, to The Editor, which suggests a connection between this community and the fight for justice (excerpts [11] and [21]).

### q28 — Which organisations was the author involved in?

The author was a dedicated member of various organizations.

* The Teachers League of South Africa (TLSA) [1]
* The Non-European Unity Movement (NEUM) [7], [17]
* The New Era Fellowship (NEF) [3], [5]

These organizations aimed to promote the rights and interests of non-European communities in South Africa, particularly during the apartheid era.

### q29 — What was the relationship between the TLSA and the Non-European Unity Movement?

According to excerpts [3] and part of excerpt [18], it appears that there was an initial split in the movement, but later they worked together. Excerpt [10] mentions that the author was drawn to the Non-European Unity Movement as a teacher and tried to make meaningful contact with people through the TLSA.

Excerpt [3]: s  our  exultation.  The  split  in  the  movement  was  put  behind  us  and  we 
worked  with  increasing  dedication  to  spread  the  Parent  Teachers’  Association  movement 
throughout the Cape. 

Excerpt [10]: It was to this 
movement that I was drawn when I began my teaching career and where I threw my energies in 
the attempt to ‘take a nation to school’, an aphorism that aptly captured the role of the Movement 
during those years.

There is no further detail regarding their relationship beyond an initial split. 

It's worth noting that Excerpt [18] discusses Ben Kies' involvement with Meltzer at Chapel Street Primary but while it mentions movement and Kies, does not directly mention a connection between TLSA and Non-European Unity Movement explicitly.

### q30 — When did J.M.H. Gool arrive in Cape Town and from where?

[1] Haji Joosub Maulvi Hamid Gool was the author's maternal grandfather, a prosperous merchant from India who arrived at the Cape via Mauritius in 1884.

### q31 — What was the Hanaffi Quwatul Islam Mosque?

The Hanafifi Quwatul Islam Mosque was an outpost of Islam in an area becoming increasingly European, Jewish, and gentile. It was founded by Haji Joosub Maulvi Hamid Gool and completed in 1898 (source [1]). The mosque was a whitewashed structure with a domed minaret and a spire with a moon and star from which the muezzin called the faithful to prayer, careful not to be too strident for fear of annoying European residents.

### q32 — How was Cissie Gool related to J.M.H. Gool?

According to [17], M.S (the father of the narrator) had a sister named Annie, who married Aity (Mohamed Saaid Gool). Their daughters were Nasima and her identical twins Nurjehan and Zarina.

In [8], Dr. Abdurahman's daughter Cissie married someone, likely Abdul Hamid, but it doesn't explicitly state his relationship to J.M.H. Gool.

However, according to the relationships established in the sources, Dr. Abdurahman was a relative (likely an uncle) of J.M.H. Gool [2]. 

In [20], Cissie Gool is described as working with Dr. A Abdurahman and thus also implies she might have been her aunt-in-law or another close family relation.

No direct information on Cissie being married to Abdul Hamid from the sources [1] or [7].

The provided sources do not clearly state how Cissie Gool was specifically related to J.M.H. Gool, but based on family relationships revealed in [8] and further inferred relationship between Dr Abdurahman's clan & the Ta’Als with the Dollie family in [12], it appears that Cissie, Abdurahman,  Saaid Gool, Aithey or M S might have been relatives.

### q33 — Who were the notable historical figures that J.M.H. Gool knew personally?

Haji Joosub Maulvi Hamid Gool knew Gandhi [4], Cecil Rhodes [1], and Dr. Abdullah Abdurahman [12] personally [1]. 

The sources do not contain more information about who else the author's grandfather, J.M.H. Gool, knew personally.

### q34 — What was the Group Areas Act and how did it affect District Six?

The Group Areas Act was a law passed by the apartheid government in South Africa that forced the relocation of non-white residents from urban areas designated as "white" and relocated them to the outskirts of cities, essentially ghettoizing them.

According to sources [1] and [10], the Group Areas Act was implemented in the 1950s and led to the displacement of an entire community. District Six, a vibrant inner-city neighborhood of Cape Town, South Africa, was affected severely by this law.

The area is described as being "haemorrhaging" gradually but definitely (source [10]). The government worked on an age-old theory that people tend to anticipate the wishes of the authorities (source [7]).

The Group Areas Act forced residents out of District Six and moved them to the Cape Flats, an impoverished area far from the city center, effectively isolating them from their livelihoods and social networks. This act decimated the once-thriving community and way of life in District Six (source [17]).

### q35 — Who was Hassen Mall?

Hassen Mall was a student from Durban who came to Cape Town to study medicine [7]. He is described as a "slim, handsome green-eyed young man" with neatly slicked back hair [1]. He became a close friend of the author and had a profound impact on his life. Hassen Mall was also a cricket player and later became a lawyer, completing his LLB degree [12].

### q36 — What political organisations were active in the Cape Coloured community during the author's lifetime?

According to excerpt [2], "the forerunner of the Coloured Affairs Department was boycotted out of existence" by a revolt led by the New Era Fellowship and the Teachers' League of South Africa. 

Excerpt [4] mentions that Cassim Amra, a member of the Communist Party, was a thorn in Ismail's side, indicating that the Communist Party was active in the Cape Coloured community.

In excerpt [9], The Non-White people, led by the New Era Union Movement (NEUM), decided to have a united front against the government's proposal. The TARC (Train Apartheid Resistance Committee) was also inaugurated as part of this effort, and all bona-fide organisations were invited to participate.

The Coloured Advisory Council (CAC) is mentioned in excerpts [2], [4], and [18]. However, it appears that some members of the Cape Coloured community saw them as "quislings" for supporting the government.

### q37 — Who was Mahatma Gandhi and why was he in South Africa?

The provided sources contain several references to Mahatma Gandhi (e.g., [3], [15], [19]). According to the sources:

Mahatma Gandhi is a well-known figure, referred to as “Mahatma” or Great Spirit ([3]).

In 1893, Gandhi came to South Africa on a one-year assignment from Dada Abdullah ([16]) to assist his attorney in a civil suit against a merchant in Pretoria. This was because the accounts were kept in Gujarati, Gandhi's mother tongue ([15]).

### q38 — Who was Cissie Gool's father?

Cissie Gool's father was Dr. Abdullah Abdurahman [1].

### q39 — What was District Six like as a neighbourhood before the forced removals?

The provided sources paint a vivid picture of District Six as a vibrant and cosmopolitan neighborhood in Cape Town. According to [1], it was home to thousands of residents, including families such as the Rassool and Gool families. The neighborhood had a "rich history of political struggle against the iniquities of the Apartheid state" ([10]).

As described in [5], District Six was a place where people came together to celebrate life. It was a neighborhood that valued community, with the author noting that those bare bulldozed acres "had more life, more zest, more fire, more warmth and care than many other places one could mention." The area was home to "a thriving cosmopolitan area in the heart of Cape Town" ([16]).

The neighborhood had a diverse population, with people from various racial and cultural backgrounds living together. As noted by [6], there was a community of freed slaves from America and the West Indies that thrived in District Six during Victorian times. The author fondly remembers growing up in this environment, describing it as a place where "life, more zest, more fire, more warmth and care" ([11]).

The neighborhood was also known for its unique culture, with people often referred to it as having a special flavour [20]. Some of the residents were described as eccentrics who added to the area's charm. However, despite its vibrancy, District Six faced significant challenges under Apartheid, including forced removals and poverty.

Overall, according to the provided sources, District Six was a vibrant and culturally rich neighborhood that was home to a diverse population before it was forcibly removed by the Apartheid government.

### q40 — What was the Unity Movement's boycott policy?

[3] mentions "He was visibly shaken by the total solidarity of the boycott." and speaks to a scene with stickers reading “Boycott the CAD men” on the school, indicating an effort by the Non-European Unity Movement to gather support for boycotting someone associated with the Coloured Affairs Department (CAD).

[12] states "The  abject  collapse  of  the Train  Apartheid  Resistance  revealed the  limitations  of  the  boycott weapon as a means of struggle against Apartheid." and goes on to explain that it was ineffective for boycotting the Group Areas Act, but notes "To those who owned property it was a different matter."

[14] indicates the Unity Movement held meetings, including one discussed in [13] which mentions "the boycott campaign of the Unity Movement" leading the Stakesby-Lewis Hostel to terminate its permission for the group to meet there.

The exact policy isn't clearly documented but it seems to have included boycotting government-backed institutions or figures.

