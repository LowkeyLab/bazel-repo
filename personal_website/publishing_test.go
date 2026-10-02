package personalwebsite

import (
	"encoding/json"
	"encoding/xml"
	"maps"
	"net/http"
	"net/url"
	"strings"
	"testing"
	"time"

	"golang.org/x/net/html"
)

const (
	siteOrigin = "https://www.tacascer.com"
	draftRoute = "/blog/rust-is-a-mind-virus/"
)

type rssItem struct {
	Title       string `xml:"title"`
	Description string `xml:"description"`
	Link        string `xml:"link"`
	GUID        string `xml:"guid"`
	PubDate     string `xml:"pubDate"`
}
type rssDocument struct {
	XMLName xml.Name  `xml:"rss"`
	Items   []rssItem `xml:"channel>item"`
}

func TestInternalLinksResolve(t *testing.T) {
	client, files, _ := startCaddy(t)
	checked := map[string]bool{}
	for pagePath, body := range files {
		if !strings.HasSuffix(pagePath, "/index.html") {
			continue
		}
		doc, err := html.Parse(strings.NewReader(body))
		if err != nil {
			t.Fatal(err)
		}
		var visit func(*html.Node)
		visit = func(node *html.Node) {
			if node.Type == html.ElementNode {
				for _, attr := range node.Attr {
					candidates := []string{}
					switch attr.Key {
					case "href", "src":
						candidates = append(candidates, attr.Val)
					case "srcset":
						for _, entry := range strings.Split(attr.Val, ",") {
							if fields := strings.Fields(entry); len(fields) > 0 {
								candidates = append(candidates, fields[0])
							}
						}
					}
					for _, raw := range candidates {
						if !strings.HasPrefix(raw, "/") || strings.HasPrefix(raw, "//") || checked[raw] {
							continue
						}
						checked[raw] = true
						parsed, err := url.Parse(raw)
						if err != nil {
							t.Errorf("%s: invalid internal URL %q: %v", pagePath, raw, err)
							continue
						}
						response, _ := get(t, client, parsed.RequestURI(), nil)
						if response.StatusCode == http.StatusMovedPermanently {
							destination := response.Header.Get("Location")
							if !strings.HasPrefix(destination, "/") {
								t.Errorf("%s: redirect %q has unexpected destination %q", pagePath, raw, destination)
								continue
							}
							response, _ = get(t, client, destination, nil)
						}
						if response.StatusCode != http.StatusOK {
							t.Errorf("%s: internal URL %q resolves to HTTP %d", pagePath, raw, response.StatusCode)
						}
					}
				}
			}
			for child := node.FirstChild; child != nil; child = child.NextSibling {
				visit(child)
			}
		}
		visit(doc)
	}
	if len(checked) == 0 {
		t.Fatal("no internal links found in rendered pages")
	}
}

func TestListingHeadingLevels(t *testing.T) {
	_, files, _ := startCaddy(t)
	for path, want := range map[string]string{"/index.html": "h3", "/blog/index.html": "h2"} {
		page := parsePage(t, files[path])
		if len(page.entryHeadings) == 0 {
			t.Fatalf("%s has no post headings", path)
		}
		for _, got := range page.entryHeadings {
			if got != want {
				t.Errorf("%s post heading = %s; want %s", path, got, want)
			}
		}
	}
}

func TestPublicationDates(t *testing.T) {
	_, files, _ := startCaddy(t)
	count := 0
	for path, body := range files {
		if !strings.HasSuffix(path, "/index.html") {
			continue
		}
		for _, date := range parsePage(t, body).dates {
			instant, err := time.Parse(time.RFC3339, date.datetime)
			if err != nil {
				t.Fatalf("%s: %v", path, err)
			}
			want := instant.UTC().Format("January 2, 2006")
			if date.text != want {
				t.Errorf("%s publication date = %q; want %q", path, date.text, want)
			}
			count++
		}
	}
	if count == 0 {
		t.Fatal("no publication dates checked")
	}
}

func TestPublishedDiscovery(t *testing.T) {
	client, files, _ := startCaddy(t)
	published := publishedRoutes(t, files)
	var feed rssDocument
	response, body := get(t, client, "/rss.xml", nil)
	if response.StatusCode != http.StatusOK {
		t.Fatalf("RSS status = %d; want 200", response.StatusCode)
	}
	if !strings.Contains(response.Header.Get("Content-Type"), "xml") {
		t.Error("RSS must have XML content type")
	}
	if err := xml.Unmarshal([]byte(body), &feed); err != nil {
		t.Fatal(err)
	}
	got := map[string]bool{}
	for _, item := range feed.Items {
		if got[item.Link] {
			t.Errorf("duplicate RSS link %s", item.Link)
		}
		got[item.Link] = true
		page := parsePage(t, files[strings.TrimPrefix(item.Link, siteOrigin)+"index.html"])
		if item.Title != page.heading || item.Description != page.meta["description"] {
			t.Errorf("RSS metadata differs from article %s", item.Link)
		}
		if item.GUID != item.Link {
			t.Errorf("GUID %q differs from canonical link %q", item.GUID, item.Link)
		}
		date, err := time.Parse(time.RFC1123Z, item.PubDate)
		if err != nil {
			date, err = time.Parse(time.RFC1123, item.PubDate)
		}
		if err != nil {
			t.Errorf("invalid publication date: %v", err)
			continue
		}
		renderedDate, err := time.Parse(time.RFC3339, page.date)
		if err != nil || !date.Equal(renderedDate) {
			t.Errorf("RSS date %s differs from publication date %s", item.PubDate, page.date)
		}
		_, offset := date.Zone()
		if offset != 0 {
			t.Error("RSS publication date must use UTC")
		}
	}
	if !maps.Equal(got, published) {
		t.Errorf("RSS routes = %v; want %v", got, published)
	}
	var sitemap struct {
		XMLName xml.Name `xml:"http://www.sitemaps.org/schemas/sitemap/0.9 urlset"`
		URLs    []string `xml:"url>loc"`
	}
	response, body = get(t, client, "/sitemap.xml", nil)
	if response.StatusCode != http.StatusOK {
		t.Fatalf("sitemap status = %d; want 200", response.StatusCode)
	}
	if !strings.Contains(response.Header.Get("Content-Type"), "xml") {
		t.Error("sitemap must have XML content type")
	}
	if err := xml.Unmarshal([]byte(body), &sitemap); err != nil {
		t.Fatal(err)
	}
	want := maps.Clone(published)
	for _, route := range []string{"/", "/blog/", "/about/"} {
		want[siteOrigin+route] = true
	}
	got = map[string]bool{}
	for _, url := range sitemap.URLs {
		if got[url] {
			t.Errorf("duplicate sitemap URL %s", url)
		}
		got[url] = true
	}
	if !maps.Equal(got, want) {
		t.Errorf("sitemap routes = %v; want %v", got, want)
	}
	for _, endpoint := range []string{"/rss.xml", "/sitemap.xml"} {
		t.Run(endpoint+" revalidation", func(t *testing.T) { checkRevalidation(t, client, endpoint, files[endpoint]) })
	}
	if !strings.Contains(files["/robots.txt"], "Sitemap: "+siteOrigin+"/sitemap.xml") {
		t.Error("robots lacks sitemap declaration")
	}
}

func TestPageMetadata(t *testing.T) {
	_, files, _ := startCaddy(t)
	published := publishedRoutes(t, files)
	for path, body := range files {
		if !strings.HasSuffix(path, "/index.html") {
			continue
		}
		t.Run(path, func(t *testing.T) {
			page := parsePage(t, body)
			canonical := siteOrigin + strings.TrimSuffix(path, "index.html")
			if page.canonical != canonical {
				t.Errorf("canonical = %q; want %q", page.canonical, canonical)
			}
			if page.meta["og:url"] != canonical || page.meta["og:description"] != page.meta["description"] || page.meta["og:description"] == "" {
				t.Errorf("missing or mismatched OG URL/description: %v", page.meta)
			}
			wantType, wantTitle := "website", page.title
			if published[canonical] {
				wantType, wantTitle = "article", page.heading
			}
			if page.meta["og:type"] != wantType || page.meta["og:title"] != wantTitle {
				t.Errorf("OG type/title = %q/%q; want %q/%q", page.meta["og:type"], page.meta["og:title"], wantType, wantTitle)
			}
			if page.feed != siteOrigin+"/rss.xml" {
				t.Errorf("feed discovery = %q", page.feed)
			}
			if !published[canonical] {
				if len(page.structured) != 0 {
					t.Error("non-published page has article structured data")
				}
				return
			}
			if len(page.structured) != 1 {
				t.Fatalf("BlogPosting count = %d; want 1", len(page.structured))
			}
			var article struct {
				Context       string `json:"@context"`
				Type          string `json:"@type"`
				Headline      string `json:"headline"`
				Description   string `json:"description"`
				DatePublished string `json:"datePublished"`
				URL           string `json:"url"`
				Author        struct {
					Type string `json:"@type"`
					Name string `json:"name"`
				} `json:"author"`
			}
			if err := json.Unmarshal([]byte(page.structured[0]), &article); err != nil {
				t.Fatal(err)
			}
			if article.Context != "https://schema.org" || article.Type != "BlogPosting" || article.Headline != page.heading || article.Description != page.meta["description"] || article.DatePublished != page.date || article.URL != canonical || article.Author.Type != "Person" || article.Author.Name != "Tim Tran" {
				t.Errorf("incorrect BlogPosting: %+v", article)
			}
		})
	}
}

func TestDraftNoindex(t *testing.T) {
	client, files, _ := startCaddy(t)
	response, body := get(t, client, draftRoute, nil)
	if response.StatusCode != http.StatusOK {
		t.Fatalf("draft status = %d; want direct access", response.StatusCode)
	}
	page := parsePage(t, body)
	if page.meta["robots"] != "noindex" {
		t.Errorf("draft robots = %q; want noindex", page.meta["robots"])
	}
	if len(page.structured) != 0 {
		t.Error("draft has published BlogPosting metadata")
	}
	for _, path := range []string{"/index.html", "/blog/index.html"} {
		for _, link := range parsePage(t, files[path]).links {
			if strings.TrimSuffix(link, "/") == strings.TrimSuffix(draftRoute, "/") {
				t.Errorf("draft listed in %s", path)
			}
		}
	}
}

func publishedRoutes(t *testing.T, files map[string]string) map[string]bool {
	t.Helper()
	routes := map[string]bool{}
	for _, link := range parsePage(t, files["/blog/index.html"]).links {
		if strings.HasPrefix(link, "/blog/") && link != "/blog/" {
			if strings.TrimSuffix(link, "/") == strings.TrimSuffix(draftRoute, "/") {
				t.Fatal("draft in public blog")
			}
			routes[siteOrigin+strings.TrimSuffix(link, "/")+"/"] = true
		}
	}
	for _, slug := range []string{"building-jvm-rpc-tooling", "prototyping-a-flink-pipeline", "making-a-platform-buildable-again", "guess-the-word", "free-dsl", "landing-page", "mindreadr", "local-first-gradle-build-scan", "the-future-is-remote"} {
		if !routes[siteOrigin+"/blog/"+slug+"/"] {
			t.Errorf("published article missing: %s", slug)
		}
	}
	return routes
}

type renderedDate struct {
	datetime, text string
}

type renderedPage struct {
	entryHeadings                         []string
	dates                                 []renderedDate
	title, heading, date, canonical, feed string
	meta                                  map[string]string
	structured, links                     []string
}

func parsePage(t *testing.T, body string) renderedPage {
	t.Helper()
	doc, err := html.Parse(strings.NewReader(body))
	if err != nil {
		t.Fatal(err)
	}
	page := renderedPage{meta: map[string]string{}}
	var content func(*html.Node) string
	content = func(n *html.Node) string {
		if n.Type == html.TextNode {
			return n.Data
		}
		var s strings.Builder
		for c := n.FirstChild; c != nil; c = c.NextSibling {
			s.WriteString(content(c))
		}
		return s.String()
	}
	var visit func(*html.Node)
	visit = func(n *html.Node) {
		if n.Type == html.ElementNode {
			attrs := map[string]string{}
			for _, a := range n.Attr {
				attrs[a.Key] = a.Val
			}
			switch n.Data {
			case "title":
				page.title = content(n)
			case "h1":
				page.heading = content(n)
			case "h2", "h3":
				if n.Parent != nil {
					for _, attr := range n.Parent.Attr {
						if attr.Key == "class" && strings.Contains(" "+attr.Val+" ", " entry-heading ") {
							page.entryHeadings = append(page.entryHeadings, n.Data)
						}
					}
				}
			case "time":
				page.date = attrs["datetime"]
				page.dates = append(page.dates, renderedDate{attrs["datetime"], strings.TrimSpace(content(n))})
			case "meta":
				key := attrs["name"]
				if key == "" {
					key = attrs["property"]
				}
				page.meta[key] = attrs["content"]
			case "a":
				page.links = append(page.links, attrs["href"])
			case "link":
				if attrs["rel"] == "canonical" {
					page.canonical = attrs["href"]
				}
				if attrs["rel"] == "alternate" && attrs["type"] == "application/rss+xml" {
					page.feed = attrs["href"]
				}
			case "script":
				if attrs["type"] == "application/ld+json" {
					page.structured = append(page.structured, content(n))
				}
			}
		}
		for c := n.FirstChild; c != nil; c = c.NextSibling {
			visit(c)
		}
	}
	visit(doc)
	return page
}
