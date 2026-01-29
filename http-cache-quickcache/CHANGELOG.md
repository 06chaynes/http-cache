<!DOCTYPE html>
<html lang="en" id="oranda" class="dark axo">
  <head>
    <title>http-cache</title>
    
    
      <link rel="icon" href="/favicon.ico" />
    
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    
      <meta name="description" content="An HTTP caching middleware" />
      <meta property="og:description" content="An HTTP caching middleware" />
    
    <meta property="og:type" content="website" />
    <meta property="og:title" content="http-cache" />
    
    
    
    <meta http-equiv="Permissions-Policy" content="interest-cohort=()" />
    <link rel="stylesheet" href="/oranda-v0.6.1.css" />
    
      <link rel="stylesheet" href="/custom.css" />
    
    
  </head>
  <body>
    <div class="container">
      <div class="page-body">
        
          <div class="repo_banner">
            <a href="https://github.com/06chaynes/http-cache">
              <div class="github-icon" aria-hidden="true"></div>
              Check out our GitHub!
            </a>
          </div>
        

        <main>
          <header>
            
            <h1 class="title">http-cache</h1>
            
  <nav class="nav">
    <ul>
      <li><a href="/">Home</a></li>

      
        
          <li><a href="/./http-cache/CHANGELOG/">http-cache changelog</a></li>
        
          <li><a href="/./http-cache-reqwest/README/">http-cache-reqwest</a></li>
        
          <li><a href="/./http-cache-reqwest/CHANGELOG/">http-cache-reqwest changelog</a></li>
        
          <li><a href="/./http-cache-surf/README/">http-cache-surf</a></li>
        
          <li><a href="/./http-cache-surf/CHANGELOG/">http-cache-surf changelog</a></li>
        
          <li><a href="/./http-cache-quickcache/README/">http-cache-quickcache</a></li>
        
          <li><a href="/./http-cache-quickcache/CHANGELOG/">http-cache-quickcache changelog</a></li>
        
      

      

      
        <li><a href="/book/">Docs</a></li>
      

      
        <li><a href="/funding/">Funding</a></li>
      

      
    </ul>
  </nav>

          </header>

          
  
    <h1>Changelog</h1>
<h2>[1.0.0-alpha.4] - 2026-01-19</h2>
<h3>Changed</h3>
<ul>
<li>Default serialization format changed from bincode to postcard (cache data incompatible with previous versions)</li>
<li>Updated <code>http-cache</code> dependency to 1.0.0-alpha.4</li>
</ul>
<h2>[1.0.0-alpha.3] - 2026-01-18</h2>
<h3>Changed</h3>
<ul>
<li>MSRV is now 1.85.0</li>
</ul>
<h2>[1.0.0-alpha.2] - 2025-08-24</h2>
<h3>Changed</h3>
<ul>
<li>Updated to use http-cache 1.0.0-alpha.2 with rate limiting support</li>
</ul>
<h2>[1.0.0-alpha.1] - 2025-07-27</h2>
<h3>Added</h3>
<ul>
<li>Integration with updated core library traits for better composability</li>
</ul>
<h3>Changed</h3>
<ul>
<li>Updated to use http-cache 1.0.0-alpha.1</li>
<li>MSRV updated to 1.82.0</li>
<li>Made <code>cache</code> field private in <code>QuickManager</code></li>
</ul>
<h2>[0.9.0] - 2025-06-25</h2>
<h3>Added</h3>
<ul>
<li><code>remove_opts</code> field to <code>CACacheManager</code> struct. This field is an instance of <code>cacache::RemoveOpts</code> that allows for customization of the removal options when deleting items from the cache.</li>
</ul>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.21.0]</li>
</ul>
</li>
</ul>
<h2>[0.8.1] - 2025-01-30</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.20.1]</li>
<li>async-trait [0.1.85]</li>
<li>darkbird [6.2.4]</li>
<li>serde [1.0.217]</li>
<li>url [2.5.4]</li>
</ul>
</li>
</ul>
<h2>[0.8.0] - 2024-11-12</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.20.0]</li>
<li>quick_cache [0.6.9]</li>
</ul>
</li>
</ul>
<h2>[0.7.0] - 2024-04-10</h2>
<h3>Changed</h3>
<ul>
<li>
<p>MSRV is now 1.71.1</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>http-cache [0.19.0]</li>
<li>http-cache-semantics [2.1.0]</li>
<li>http [1.1.0]</li>
<li>reqwest [0.12.3]</li>
<li>reqwest-middleware [0.3.0]</li>
<li>quick_cache [0.5.1]</li>
</ul>
</li>
</ul>
<h2>[0.6.3] - 2024-01-15</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.18.0]</li>
</ul>
</li>
</ul>
<h2>[0.6.2] - 2023-11-01</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.17.0]</li>
</ul>
</li>
</ul>
<h2>[0.6.1] - 2023-09-28</h2>
<h3>Changed</h3>
<ul>
<li>
<p>MSRV is now 1.67.1</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>http-cache [0.16.0]</li>
</ul>
</li>
</ul>
<h2>[0.6.0] - 2023-09-26</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.15.0]</li>
<li>quick_cache [0.4.0]</li>
</ul>
</li>
</ul>
<h2>[0.5.1] - 2023-07-28</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.14.0]</li>
<li>async-trait [0.1.72]</li>
<li>serde [1.0.178]</li>
<li>tokio [1.29.1]</li>
</ul>
</li>
</ul>
<h2>[0.5.0] - 2023-07-19</h2>
<h3>Changed</h3>
<ul>
<li>
<p><code>CacheManager</code> trait <code>get</code>, <code>put</code>, and <code>delete</code> methods now require a <code>cache_key</code> argument rather than <code>method</code> and <code>url</code> arguments. This allows for custom keys to be specified.</p>
</li>
<li>
<p>The <code>QuickManager</code> trait implementation has been updated to reflect the above change.</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>http-cache [0.13.0]</li>
<li>async-trait [0.1.71]</li>
<li>serde [1.0.171]</li>
</ul>
</li>
</ul>
<h2>[0.4.0] - 2023-06-05</h2>
<h3>Changed</h3>
<ul>
<li>MSRV is now 1.66.1</li>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.12.0]</li>
<li>serde [1.0.163]</li>
<li>quick_cache [0.3.0]</li>
<li>url [2.4.0]</li>
</ul>
</li>
</ul>
<h2>[0.3.0] - 2023-03-29</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.11.0]</li>
<li>async-trait [0.1.68]</li>
<li>serde [1.0.159]</li>
<li>quick_cache [0.2.4]</li>
</ul>
</li>
</ul>
<h2>[0.2.0] - 2023-03-08</h2>
<h3>Changed</h3>
<ul>
<li>
<p>MSRV is now 1.63.0</p>
</li>
<li>
<p>Set <code>default-features = false</code> for <code>http-cache</code> dependency</p>
</li>
<li>
<p>Updated the minimum versions of the following dependencies:</p>
<ul>
<li>http-cache [0.10.1]</li>
<li>async-trait [0.1.66]</li>
<li>serde [1.0.154]</li>
</ul>
</li>
</ul>
<h2>[0.1.2] - 2023-02-23</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>http-cache [0.9.2]</li>
<li>quick_cache [0.2.2]</li>
</ul>
</li>
</ul>
<h2>[0.1.1] - 2023-02-17</h2>
<h3>Changed</h3>
<ul>
<li>Updated the minimum versions of the following dependencies:
<ul>
<li>quick_cache [0.2.1]</li>
</ul>
</li>
</ul>
<h2>[0.1.0] - 2023-02-16</h2>
<h3>Added</h3>
<ul>
<li>Initial release</li>
</ul>

  

        </main>
      </div>

      <footer>
        
          <a href="https://github.com/06chaynes/http-cache"><div class="github-icon" aria-hidden="true"></div></a>
        
        <span>
          http-cache, MIT OR Apache-2.0
        </span>
      </footer>
    </div>

    
    

    
  </body>
</html>