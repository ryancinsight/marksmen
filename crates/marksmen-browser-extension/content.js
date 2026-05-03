// Extract Highwire Press / Google Scholar metadata tags
function extractMetadata() {
  const metaTags = document.getElementsByTagName('meta');
  const metadata = {
    title: '',
    authors: [],
    abstract_text: '',
    journal: '',
    year: '',
    doi: '',
    url: window.location.href,
    source: 'browser_extension'
  };

  for (let i = 0; i < metaTags.length; i++) {
    const name = metaTags[i].getAttribute('name') || metaTags[i].getAttribute('property');
    const content = metaTags[i].getAttribute('content');

    if (!name || !content) continue;

    const n = name.toLowerCase();

    if (n === 'citation_title' || n === 'dc.title' || n === 'og:title') {
      if (!metadata.title) metadata.title = content;
    } else if (n === 'citation_author' || n === 'dc.creator') {
      metadata.authors.push(content);
    } else if (n === 'citation_journal_title' || n === 'prism.publicationname') {
      if (!metadata.journal) metadata.journal = content;
    } else if (n === 'citation_publication_date' || n === 'citation_date' || n === 'dc.date') {
      if (!metadata.year) {
        // Try to extract just the year (YYYY)
        const match = content.match(/\d{4}/);
        if (match) metadata.year = match[0];
        else metadata.year = content;
      }
    } else if (n === 'citation_doi' || n === 'dc.identifier') {
      if (content.startsWith('10.')) {
        metadata.doi = content;
      } else if (content.includes('doi.org/')) {
        metadata.doi = content.split('doi.org/')[1];
      }
    } else if (n === 'citation_abstract' || n === 'dc.description' || n === 'og:description') {
      if (!metadata.abstract_text) metadata.abstract_text = content;
    }
  }

  // Fallback for title if no meta tag
  if (!metadata.title) {
    metadata.title = document.title;
  }

  return metadata;
}

// Send the extracted metadata back to the popup script
chrome.runtime.onMessage.addListener((request, sender, sendResponse) => {
  if (request.action === "extract_metadata") {
    sendResponse(extractMetadata());
  }
});
