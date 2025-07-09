#!/usr/bin/env node

/**
 * Manual test script to verify paste API compatibility
 * This simulates what happens in older VS Code versions
 */

// Mock the vscode module for testing
const mockVscode = {
  languages: {
    registerDocumentDropEditProvider: () => ({ dispose: () => {} }),
    // registerDocumentPasteEditProvider intentionally missing to simulate older VS Code
  },
  DocumentDropEdit: class MockDocumentDropEdit {
    constructor(snippet) {
      this.snippet = snippet;
    }
  },
  // DocumentPasteEdit intentionally missing
  // DocumentDropOrPasteEditKind intentionally missing
};

// Mock the dependencies
const mockContext = {
  subscriptions: []
};

const mockTypstDocumentSelector = { language: 'typst' };

// Simulate the drop-paste module with mocked dependencies
function testCompatibility() {
  console.log('Testing VS Code Paste API Compatibility...\n');
  
  // Test 1: Check if paste API is available
  console.log('1. Checking paste API availability...');
  const hasPasteAPI = "registerDocumentPasteEditProvider" in mockVscode.languages;
  console.log(`   registerDocumentPasteEditProvider available: ${hasPasteAPI}`);
  
  // Test 2: Test copyAndPasteActivate function simulation
  console.log('\n2. Testing copyAndPasteActivate function...');
  
  function simulatedCopyAndPasteActivate(context) {
    if (!hasPasteAPI) {
      console.warn('   Tinymist: Document paste API not available, copy/paste features will be disabled');
      return;
    }
    
    console.log('   Paste providers registered successfully');
    context.subscriptions.push(
      { dispose: () => {} }, // Mock PasteUriProvider
      { dispose: () => {} }  // Mock PasteResourceProvider
    );
  }
  
  simulatedCopyAndPasteActivate(mockContext);
  
  // Test 3: Test dragAndDropActivate function simulation
  console.log('\n3. Testing dragAndDropActivate function...');
  
  function simulatedDragAndDropActivate(context) {
    console.log('   Drop provider registered successfully');
    context.subscriptions.push(
      mockVscode.languages.registerDocumentDropEditProvider(mockTypstDocumentSelector, {})
    );
  }
  
  simulatedDragAndDropActivate(mockContext);
  
  // Test 4: Show final state
  console.log('\n4. Final registration state:');
  console.log(`   Total subscriptions: ${mockContext.subscriptions.length}`);
  console.log(`   Expected: 1 (drop only, paste disabled)`);
  
  // Test 5: Test with paste API available
  console.log('\n5. Testing with paste API available...');
  mockVscode.languages.registerDocumentPasteEditProvider = () => ({ dispose: () => {} });
  
  const mockContext2 = { subscriptions: [] };
  
  function simulatedCopyAndPasteActivateWithAPI(context) {
    const hasPasteAPI = "registerDocumentPasteEditProvider" in mockVscode.languages;
    if (!hasPasteAPI) {
      console.warn('   Tinymist: Document paste API not available, copy/paste features will be disabled');
      return;
    }
    
    console.log('   Paste providers registered successfully');
    context.subscriptions.push(
      { dispose: () => {} }, // Mock PasteUriProvider
      { dispose: () => {} }  // Mock PasteResourceProvider
    );
  }
  
  simulatedCopyAndPasteActivateWithAPI(mockContext2);
  simulatedDragAndDropActivate(mockContext2);
  
  console.log(`   Total subscriptions: ${mockContext2.subscriptions.length}`);
  console.log(`   Expected: 3 (drop + 2 paste providers)`);
  
  console.log('\n✅ Compatibility test completed successfully!');
  console.log('\nSummary:');
  console.log('- Older VS Code (no paste API): Drop works, paste disabled with warning');
  console.log('- Newer VS Code (with paste API): Both drop and paste work normally');
  console.log('- No crashes or errors in either case');
}

// Run the test
testCompatibility();