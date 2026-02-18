import TokenMintForm from '../../components/tokens/TokenMintForm';

export default function TokenMint() {
  return (
    <div style={{ maxWidth: 500 }}>
      <h1>Mint Token</h1>
      <TokenMintForm idPrefix="mint" />
    </div>
  );
}
