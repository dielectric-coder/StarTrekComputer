#!/usr/bin/env python3
import sys
import requests
from bs4 import BeautifulSoup
import json
import subprocess
import time

def search_bing(query, num_results=3):
    """Search using Bing through Firefox for fact-checking"""
    try:
        headers = {
            'User-Agent': 'Mozilla/5.0 (X11; Linux aarch64; rv:91.0) Gecko/20100101 Firefox/91.0'
        }
        
        # Use Bing search
        url = f"https://www.bing.com/search?q={query}"
        response = requests.get(url, headers=headers, timeout=10)
        
        soup = BeautifulSoup(response.text, 'html.parser')
        results = []
        
        # Parse Bing search results
        for result in soup.find_all('li', class_='b_algo')[:num_results]:
            title_elem = result.find('h2')
            link_elem = title_elem.find('a') if title_elem else None
            snippet_elem = result.find('div', class_='b_caption')
            
            if link_elem and snippet_elem:
                title = title_elem.get_text(strip=True)
                url = link_elem.get('href')
                snippet_text = snippet_elem.get_text(strip=True)
                
                results.append({
                    'title': title,
                    'url': url,
                    'snippet': snippet_text,
                    'source': 'bing'
                })
        
        return results
    except Exception as e:
        return [{'error': f'Bing search failed: {str(e)}', 'source': 'bing'}]

def fact_check_search(query, num_results=2):
    """Search fact-checking sources for verification"""
    fact_check_sources = [
        "site:factcheck.org",
        "site:snopes.com",
        "site:politifact.com",
        "site:reuters.com/fact-check",
        "site:apnews.com/hub/ap-fact-check"
    ]
    
    results = []
    for source in fact_check_sources[:2]:  # Limit to prevent overwhelming
        try:
            enhanced_query = f"{query} {source}"
            fact_results = search_bing(enhanced_query, 1)
            if fact_results and 'error' not in fact_results[0]:
                results.extend(fact_results)
        except:
            continue
    
    return results

def search_web(query, num_results=3, include_fact_check=True):
    """Main search function with fact-checking integration"""
    try:
        # Primary search
        primary_results = search_bing(query, num_results)
        
        # Fact-checking search for verification
        fact_check_results = []
        if include_fact_check:
            fact_check_results = fact_check_search(query, 2)
        
        # Combine results
        all_results = {
            'primary_search': primary_results,
            'fact_check': fact_check_results,
            'query': query,
            'timestamp': time.time()
        }
        
        return all_results
        
    except Exception as e:
        return {
            'primary_search': [{'error': str(e)}],
            'fact_check': [],
            'query': query,
            'timestamp': time.time()
        }

def firefox_search(query):
    """Launch Firefox with search query as fallback"""
    try:
        search_url = f"https://www.bing.com/search?q={query}"
        subprocess.Popen(['firefox', search_url], 
                        stdout=subprocess.DEVNULL, 
                        stderr=subprocess.DEVNULL)
        return [{'message': f'Launched Firefox search for: {query}', 'url': search_url}]
    except Exception as e:
        return [{'error': f'Firefox launch failed: {str(e)}'}]

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(json.dumps({'error': 'No query provided'}, indent=2))
        sys.exit(1)
    
    query = " ".join(sys.argv[1:])
    
    # Check if this is a fact-check request
    if '--fact-check' in sys.argv:
        query = query.replace('--fact-check', '').strip()
        results = fact_check_search(query)
    elif '--firefox' in sys.argv:
        query = query.replace('--firefox', '').strip()
        results = firefox_search(query)
    else:
        results = search_web(query, include_fact_check=True)
    
    print(json.dumps(results, indent=2))
