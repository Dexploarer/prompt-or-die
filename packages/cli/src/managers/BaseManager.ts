/**
 * @fileoverview Base manager class for CLI data management
 * @module @prompt-or-die/cli/managers/BaseManager
 */

import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs';
import { join } from 'path';
import chalk from 'chalk';

/**
 * Base manager class for handling file operations
 */
export abstract class BaseManager<T extends { id: string }> {
  protected dataDir: string;
  protected filename: string;
  protected filePath: string;

  constructor(dataDir: string, filename: string) {
    this.dataDir = dataDir;
    this.filename = filename;
    this.filePath = join(dataDir, filename);
    this.ensureDataDir();
  }

  /**
   * Ensure data directory exists
   */
  protected ensureDataDir(): void {
    if (!existsSync(this.dataDir)) {
      mkdirSync(this.dataDir, { recursive: true });
    }
  }

  /**
   * Load data from file
   */
  protected loadData(): T[] {
    if (!existsSync(this.filePath)) {
      return [];
    }
    
    try {
      const data = readFileSync(this.filePath, 'utf8');
      return JSON.parse(data);
    } catch (error: unknown) {
      console.warn(chalk.yellow(`⚠️  Failed to load ${this.filename}:`, error instanceof Error ? error.message : String(error)));
      return [];
    }
  }

  /**
   * Save data to file
   */
  protected saveData(data: T[]): void {
    try {
      writeFileSync(this.filePath, JSON.stringify(data, null, 2));
    } catch (error: unknown) {
      console.error(chalk.red(`❌ Failed to save ${this.filename}:`, error instanceof Error ? error.message : String(error)));
      throw error;
    }
  }

  /**
   * Save single item
   */
  save(item: T & { id: string }): void {
    const data = this.loadData();
    const existingIndex = data.findIndex((existing: T & { id: string }) => existing.id === item.id);
    
    if (existingIndex >= 0) {
      data[existingIndex] = item;
    } else {
      data.push(item);
    }
    
    this.saveData(data);
  }

  /**
   * Load all items
   */
  loadAll(): T[] {
    return this.loadData();
  }

  /**
   * Find item by ID
   */
  findById(id: string): T | undefined {
    const data = this.loadData();
    return data.find((item: T & { id: string }) => item.id === id);
  }

  /**
   * Delete item by ID
   */
  delete(id: string): boolean {
    const data = this.loadData();
    const initialLength = data.length;
    const filteredData = data.filter((item: T & { id: string }) => item.id !== id);
    
    if (filteredData.length < initialLength) {
      this.saveData(filteredData);
      return true;
    }
    
    return false;
  }

  /**
   * Get count of items
   */
  count(): number {
    return this.loadData().length;
  }

  /**
   * Search items by name or description
   */
  search(query: string): T[] {
    const data = this.loadData();
    const lowerQuery = query.toLowerCase();
    
    return data.filter((item: T & { name?: string; description?: string }) => {
      const name = item.name?.toLowerCase() || '';
      const description = item.description?.toLowerCase() || '';
      return name.includes(lowerQuery) || description.includes(lowerQuery);
    });
  }

  /**
   * Export all data
   */
  exportData(): T[] {
    return this.loadData();
  }

  /**
   * Import data
   */
  importData(data: T[]): void {
    // Validate that data is an array before saving
    if (!Array.isArray(data)) {
      throw new Error('Import data must be an array');
    }
    
    this.saveData(data);
  }
}