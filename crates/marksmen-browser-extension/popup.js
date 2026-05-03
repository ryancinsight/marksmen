let currentMetadata = null;

document.addEventListener('DOMContentLoaded', () => {
  const btnSave = document.getElementById('btn-save');
  const errDiv = document.getElementById('status-error');
  const okDiv = document.getElementById('status-success');

  // Query the active tab
  chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
    if (tabs.length === 0) return;
    
    // Request metadata from content.js
    chrome.tabs.sendMessage(tabs[0].id, { action: "extract_metadata" }, (response) => {
      if (chrome.runtime.lastError) {
        errDiv.textContent = "Could not extract metadata. Refresh the page.";
        errDiv.style.display = 'block';
        return;
      }
      
      if (response) {
        currentMetadata = response;
        document.getElementById('val-title').textContent = response.title || 'Unknown Title';
        document.getElementById('val-authors').textContent = response.authors && response.authors.length > 0 
          ? response.authors.join(', ') : 'Unknown Authors';
        document.getElementById('val-doi').textContent = response.doi || 'No DOI found';
        
        btnSave.disabled = false;
      }
    });
  });

  // Handle Save
  btnSave.addEventListener('click', () => {
    if (!currentMetadata) return;
    
    btnSave.disabled = true;
    btnSave.textContent = "Saving...";
    errDiv.style.display = 'none';
    okDiv.style.display = 'none';

    fetch('http://127.0.0.1:14242/import', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json'
      },
      body: JSON.stringify(currentMetadata)
    })
    .then(response => {
      if (response.ok) {
        okDiv.style.display = 'block';
        btnSave.textContent = "Done";
      } else {
        throw new Error(`Server returned ${response.status}`);
      }
    })
    .catch(err => {
      errDiv.textContent = "Error saving to Marksmen: " + err.message + ". Is Marksmen Cite running?";
      errDiv.style.display = 'block';
      btnSave.disabled = false;
      btnSave.textContent = "Save to Marksmen";
    });
  });
});
