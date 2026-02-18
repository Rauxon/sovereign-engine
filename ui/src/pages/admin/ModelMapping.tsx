import { useState, useEffect, useCallback } from 'react';
import { getCategories, getAdminModels, createCategory, updateCategory, deleteCategory, updateModel } from '../../api';
import type { Category, AdminModel } from '../../types';
import { useTheme, tableStyles, formStyles } from '../../theme';
import LoadingSpinner from '../../components/common/LoadingSpinner';
import ErrorAlert from '../../components/common/ErrorAlert';
import ConfirmDialog from '../../components/common/ConfirmDialog';

export default function ModelMapping() {
  const { colors } = useTheme();
  const [categories, setCategories] = useState<Category[]>([]);
  const [models, setModels] = useState<AdminModel[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Category form
  const [showCatForm, setShowCatForm] = useState(false);
  const [editingCat, setEditingCat] = useState<Category | null>(null);
  const [catName, setCatName] = useState('');
  const [catDescription, setCatDescription] = useState('');
  const [catPreferredModel, setCatPreferredModel] = useState('');
  const [catSubmitting, setCatSubmitting] = useState(false);
  const [catSubmitError, setCatSubmitError] = useState<string | null>(null);

  // Delete confirm
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

  const { table: tableStyle, th: thStyle, td: tdStyle } = tableStyles(colors);
  const { input: inputStyle, label: labelStyle } = formStyles(colors);

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [cats, mods] = await Promise.all([getCategories(), getAdminModels()]);
      setCategories(cats);
      setModels(mods);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load data');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  const openCreateForm = () => {
    setEditingCat(null);
    setCatName('');
    setCatDescription('');
    setCatPreferredModel('');
    setCatSubmitError(null);
    setShowCatForm(true);
  };

  const openEditForm = (cat: Category) => {
    setEditingCat(cat);
    setCatName(cat.name);
    setCatDescription(cat.description);
    setCatPreferredModel(cat.preferred_model_id || '');
    setCatSubmitError(null);
    setShowCatForm(true);
  };

  const handleCatSubmit = async (e: React.SubmitEvent) => {
    e.preventDefault();
    setCatSubmitting(true);
    setCatSubmitError(null);
    try {
      const payload = {
        name: catName.trim(),
        description: catDescription.trim(),
        preferred_model_id: catPreferredModel || null,
      };
      if (editingCat) {
        await updateCategory(editingCat.id, payload);
      } else {
        await createCategory(payload);
      }
      setShowCatForm(false);
      await fetchData();
    } catch (err) {
      setCatSubmitError(err instanceof Error ? err.message : 'Failed to save category');
    } finally {
      setCatSubmitting(false);
    }
  };

  const handleDelete = async (id: string) => {
    setConfirmDelete(null);
    try {
      await deleteCategory(id);
      setCategories((prev) => prev.filter((c) => c.id !== id));
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to delete category');
    }
  };

  const handleModelCategoryChange = async (modelId: string, categoryId: string) => {
    try {
      await updateModel(modelId, { category_id: categoryId || null });
      setModels((prev) =>
        prev.map((m) => (m.id === modelId ? { ...m, category_id: categoryId || null } : m))
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to update model');
    }
  };

  const getModelName = (modelId: string | null): string => {
    if (!modelId) return '\u2014';
    const model = models.find((m) => m.id === modelId);
    return model ? model.hf_repo : modelId;
  };

  const getCatSubmitLabel = () => {
    if (catSubmitting) return 'Saving...';
    return editingCat ? 'Update' : 'Create';
  };

  if (loading) return <LoadingSpinner message="Loading model mappings..." />;
  if (error) return <ErrorAlert message={error} onRetry={fetchData} />;

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1.5rem' }}>
        <h1 style={{ margin: 0 }}>Model Mapping</h1>
        <button
          onClick={() => showCatForm ? setShowCatForm(false) : openCreateForm()}
          style={{
            padding: '0.5rem 1rem',
            background: showCatForm ? colors.buttonDisabled : colors.buttonPrimary,
            color: showCatForm ? colors.textSecondary : '#fff',
            border: 'none',
            borderRadius: 4,
            cursor: 'pointer',
          }}
        >
          {showCatForm ? 'Cancel' : 'New Category'}
        </button>
      </div>

      {showCatForm && (
        <div style={{ background: colors.cardBg, border: `1px solid ${colors.cardBorder}`, borderRadius: 8, padding: '1.5rem', marginBottom: '1.5rem', maxWidth: 500 }}>
          <h3 style={{ margin: '0 0 1rem' }}>{editingCat ? 'Edit Category' : 'New Category'}</h3>
          {catSubmitError && <ErrorAlert message={catSubmitError} />}
          <form onSubmit={handleCatSubmit}>
            <div style={{ marginBottom: '1rem' }}>
              <label htmlFor="cat-name" style={labelStyle}>Name *</label>
              <input id="cat-name" type="text" value={catName} onChange={(e) => setCatName(e.target.value)} style={inputStyle} placeholder="e.g. thinking" required />
            </div>
            <div style={{ marginBottom: '1rem' }}>
              <label htmlFor="cat-description" style={labelStyle}>Description *</label>
              <input id="cat-description" type="text" value={catDescription} onChange={(e) => setCatDescription(e.target.value)} style={inputStyle} placeholder="e.g. Models for complex reasoning" required />
            </div>
            <div style={{ marginBottom: '1rem' }}>
              <label htmlFor="cat-preferred-model" style={labelStyle}>Preferred Model</label>
              <select
                id="cat-preferred-model"
                value={catPreferredModel}
                onChange={(e) => setCatPreferredModel(e.target.value)}
                style={{ ...inputStyle, background: colors.inputBg }}
              >
                <option value="">None</option>
                {models.map((m) => (
                  <option key={m.id} value={m.id}>{m.hf_repo}</option>
                ))}
              </select>
            </div>
            <button
              type="submit"
              disabled={catSubmitting}
              style={{
                padding: '0.5rem 1.25rem',
                background: catSubmitting ? colors.buttonPrimaryDisabled : colors.buttonPrimary,
                color: '#fff',
                border: 'none',
                borderRadius: 4,
                cursor: catSubmitting ? 'default' : 'pointer',
              }}
            >
              {getCatSubmitLabel()}
            </button>
          </form>
        </div>
      )}

      {/* Categories table */}
      <h2 style={{ marginBottom: '0.75rem' }}>Categories</h2>
      {categories.length === 0 ? (
        <p style={{ color: colors.textMuted }}>No categories defined.</p>
      ) : (
        <table style={{ ...tableStyle, marginBottom: '2rem' }}>
          <thead>
            <tr>
              <th style={thStyle}>Name</th>
              <th style={thStyle}>Description</th>
              <th style={thStyle}>Preferred Model</th>
              <th style={thStyle}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {categories.map((cat) => (
              <tr key={cat.id}>
                <td style={{ ...tdStyle, fontWeight: 600 }}>{cat.name}</td>
                <td style={tdStyle}>{cat.description}</td>
                <td style={tdStyle}>{getModelName(cat.preferred_model_id)}</td>
                <td style={tdStyle}>
                  <div style={{ display: 'flex', gap: '0.5rem' }}>
                    <button
                      onClick={() => openEditForm(cat)}
                      style={{
                        padding: '0.3rem 0.7rem',
                        background: colors.buttonPrimary,
                        color: '#fff',
                        border: 'none',
                        borderRadius: 4,
                        cursor: 'pointer',
                        fontSize: '0.8rem',
                      }}
                    >
                      Edit
                    </button>
                    <button
                      onClick={() => setConfirmDelete(cat.id)}
                      style={{
                        padding: '0.3rem 0.7rem',
                        background: colors.buttonDanger,
                        color: '#fff',
                        border: 'none',
                        borderRadius: 4,
                        cursor: 'pointer',
                        fontSize: '0.8rem',
                      }}
                    >
                      Delete
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {/* Models and their category assignment */}
      <h2 style={{ marginBottom: '0.75rem' }}>Model Category Assignments</h2>
      {models.length === 0 ? (
        <p style={{ color: colors.textMuted }}>No models registered.</p>
      ) : (
        <table style={tableStyle}>
          <thead>
            <tr>
              <th style={thStyle}>Model</th>
              <th style={thStyle}>Backend</th>
              <th style={thStyle}>Status</th>
              <th style={thStyle}>Category</th>
            </tr>
          </thead>
          <tbody>
            {models.map((model) => (
              <tr key={model.id}>
                <td style={tdStyle}>{model.hf_repo}</td>
                <td style={tdStyle}>
                  <span
                    style={{
                      display: 'inline-block',
                      padding: '0.15rem 0.5rem',
                      borderRadius: 12,
                      fontSize: '0.75rem',
                      fontWeight: 600,
                      background: colors.badgeWarningBg,
                      color: colors.badgeWarningText,
                    }}
                  >
                    llama.cpp
                  </span>
                </td>
                <td style={tdStyle}>
                  <span
                    style={{
                      display: 'inline-block',
                      padding: '0.2rem 0.6rem',
                      borderRadius: 12,
                      fontSize: '0.8rem',
                      fontWeight: 600,
                      background: model.loaded ? colors.badgeSuccessBg : colors.badgeNeutralBg,
                      color: model.loaded ? colors.badgeSuccessText : colors.badgeNeutralText,
                    }}
                  >
                    {model.loaded ? 'Loaded' : 'Not Loaded'}
                  </span>
                </td>
                <td style={tdStyle}>
                  <select
                    aria-label={`Category for ${model.hf_repo}`}
                    value={model.category_id || ''}
                    onChange={(e) => handleModelCategoryChange(model.id, e.target.value)}
                    style={{ padding: '0.3rem 0.5rem', border: `1px solid ${colors.inputBorder}`, borderRadius: 4, fontSize: '0.85rem', background: colors.inputBg, color: colors.textPrimary }}
                  >
                    <option value="">Unassigned</option>
                    {categories.map((cat) => (
                      <option key={cat.id} value={cat.id}>{cat.name}</option>
                    ))}
                  </select>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {confirmDelete && (
        <ConfirmDialog
          title="Delete Category"
          message="This will remove the category. Tokens and models using it will become uncategorized."
          confirmLabel="Delete"
          destructive
          onConfirm={() => handleDelete(confirmDelete)}
          onCancel={() => setConfirmDelete(null)}
        />
      )}
    </div>
  );
}
