"""
---
id: "2b393cae-84b9-426f-b4cf-4902aea79d7d"
projectId: "1979237c-034d-41f6-99a0-f35eb57b3f6c"
slug: "bgg_link"
title: "bgg link"
kind: "transform"
entrypoint: "normalize_blogmark"
enabled: true
version: 12
createdAt: "2026-02-23T19:54:58.000Z"
updatedAt: "2026-03-02T20:30:14.453Z"
---
"""
def normalize_blogmark(post):
	title = (post.get("title") or "").strip()
	if title and "BoardGameGeek" in title:
		ntitle = title.split(" | ")[0]
		post["title"] = ntitle
		post["content"] = post["content"].replace(title, ntitle)

		post["categories"] = ["spielelog", "aside"]

		tags = post.get("tags") or []
		tags.append("spielen")
		post["tags"] = sorted({str(tag).strip().lower() for tag in tags if str(tag).strip()})

		toast(f"BGG transform applied: {post.get('title')}")
	return post