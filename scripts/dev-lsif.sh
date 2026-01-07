# Step 1
# run tests in docs/package.rs
# Step 2
typst watch 'target/preview-touying-0.6.0.typ' --features html target/test.html --root . --font-path assets/fonts --input x-target=html-ayu
# Step 3
# yarn watch:index
# Step 4
# view the html in the browser.
